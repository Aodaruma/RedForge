//! Read-only access to textures from a locally installed Minecraft client.
//!
//! RedForge deliberately does not redistribute Mojang assets. This module finds
//! a client JAR owned by the user and reads the required PNG files in place.

use std::{
    cmp::Ordering,
    collections::BTreeSet,
    env,
    error::Error,
    fmt,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

use bevy::{
    asset::RenderAssetUsages,
    image::Image,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use bevy_egui::egui;
use image::ImageFormat;
use zip::{ZipArchive, result::ZipError};

const PROBE_TEXTURE: &str = "assets/minecraft/textures/block/stone.png";

/// Set this to a client JAR when automatic launcher discovery is insufficient.
pub const CLIENT_JAR_OVERRIDE: &str = "REDFORGE_MINECRAFT_JAR";

#[derive(Debug)]
pub enum AssetError {
    NotFound,
    UnsafePath(String),
    MissingEntry(String),
    Io(std::io::Error),
    Archive(String),
    Decode(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(
                formatter,
                "Minecraft client JAR not found; install Minecraft or set {CLIENT_JAR_OVERRIDE}"
            ),
            Self::UnsafePath(path) => write!(formatter, "unsafe Minecraft asset path: {path}"),
            Self::MissingEntry(path) => {
                write!(formatter, "asset is missing from client JAR: {path}")
            }
            Self::Io(error) => error.fmt(formatter),
            Self::Archive(error) => write!(formatter, "invalid Minecraft client JAR: {error}"),
            Self::Decode(error) => write!(formatter, "invalid Minecraft PNG: {error}"),
        }
    }
}

impl Error for AssetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AssetError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaTexture {
    pub width: u32,
    pub height: u32,
    /// Unpremultiplied pixels in row-major RGBA8 order.
    pub pixels: Vec<u8>,
}

impl RgbaTexture {
    /// Minecraft animated textures are vertical strips. Palette and material
    /// previews use their first square frame.
    pub fn first_frame(self) -> Self {
        if self.height <= self.width || !self.height.is_multiple_of(self.width) {
            return self;
        }

        let byte_count = (self.width * self.width * 4) as usize;
        Self {
            width: self.width,
            height: self.width,
            pixels: self.pixels[..byte_count].to_vec(),
        }
    }

    pub fn egui_image(&self) -> egui::ColorImage {
        egui::ColorImage::from_rgba_unmultiplied(
            [self.width as usize, self.height as usize],
            &self.pixels,
        )
    }

    pub fn into_bevy_image(self) -> Image {
        Image::new(
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            self.pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteIconKind {
    /// A real flat item sprite from `textures/item`.
    ItemSprite,
    /// The item is model-rendered by Minecraft, so a representative block
    /// texture is used until RedForge has a model renderer.
    BlockTexture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteTexture {
    pub path: &'static str,
    pub kind: PaletteIconKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockTextures {
    /// Model-local top face.
    pub top: &'static str,
    /// Model-local bottom face.
    pub bottom: &'static str,
    /// Model-local side face.
    pub side: &'static str,
    /// Model-local facing/output face.
    pub front: &'static str,
    /// Model-local rear/input face.
    pub back: &'static str,
}

impl BlockTextures {
    const fn cube(path: &'static str) -> Self {
        Self {
            top: path,
            bottom: path,
            side: path,
            front: path,
            back: path,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockFace {
    Top,
    Bottom,
    Side,
    Front,
    Back,
}

impl BlockTextures {
    pub const fn for_face(self, face: BlockFace) -> &'static str {
        match face {
            BlockFace::Top => self.top,
            BlockFace::Bottom => self.bottom,
            BlockFace::Side => self.side,
            BlockFace::Front => self.front,
            BlockFace::Back => self.back,
        }
    }
}

#[derive(Debug)]
pub struct LoadedPaletteTexture {
    pub source_path: &'static str,
    pub kind: PaletteIconKind,
    pub image: RgbaTexture,
}

#[derive(Clone)]
pub struct MinecraftAssets {
    jar_path: PathBuf,
    version: String,
    archive: Arc<Mutex<ZipArchive<File>>>,
}

impl MinecraftAssets {
    /// Finds the newest usable vanilla client in official Launcher, Prism, or
    /// MultiMC storage. No asset is copied out of the JAR.
    pub fn discover() -> Result<Self, AssetError> {
        if let Some(path) = env::var_os(CLIENT_JAR_OVERRIDE) {
            return Self::open(path);
        }

        let mut candidates = discover_candidates();
        candidates.sort_by(|left, right| {
            compare_versions(&right.version, &left.version)
                .then_with(|| right.modified.cmp(&left.modified))
        });

        candidates
            .into_iter()
            .find_map(|candidate| Self::open(candidate.path).ok())
            .ok_or(AssetError::NotFound)
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AssetError> {
        let jar_path = path.into();
        let version = version_hint(&jar_path);
        let file = File::open(&jar_path)?;
        let archive =
            ZipArchive::new(file).map_err(|error| AssetError::Archive(error.to_string()))?;
        let assets = Self {
            jar_path,
            version,
            archive: Arc::new(Mutex::new(archive)),
        };
        assets.read_resource(PROBE_TEXTURE)?;
        Ok(assets)
    }

    pub fn jar_path(&self) -> &Path {
        &self.jar_path
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Reads one namespaced resource using its complete archive path.
    pub fn read_resource(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        validate_resource_path(path)?;
        let mut archive = self
            .archive
            .lock()
            .map_err(|_| AssetError::Archive("asset reader lock was poisoned".to_owned()))?;
        let mut entry = archive.by_name(path).map_err(|error| match error {
            ZipError::FileNotFound => AssetError::MissingEntry(path.to_owned()),
            other => AssetError::Archive(other.to_string()),
        })?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    pub fn load_png(&self, path: &str) -> Result<RgbaTexture, AssetError> {
        decode_png(&self.read_resource(path)?)
    }

    pub fn load_palette_texture(
        &self,
        block_state: &str,
    ) -> Result<LoadedPaletteTexture, AssetError> {
        let mapping = palette_texture(block_state).ok_or_else(|| {
            AssetError::MissingEntry(format!("palette mapping for {}", block_id(block_state)))
        })?;
        let image = match self.load_png(mapping.path) {
            Ok(image) => image,
            Err(AssetError::MissingEntry(_)) if mapping.kind == PaletteIconKind::ItemSprite => {
                let fallback = block_textures(block_state)
                    .ok_or_else(|| AssetError::MissingEntry(mapping.path.to_owned()))?
                    .top;
                return Ok(LoadedPaletteTexture {
                    source_path: fallback,
                    kind: PaletteIconKind::BlockTexture,
                    image: self.load_png(fallback)?.first_frame(),
                });
            }
            Err(error) => return Err(error),
        };

        Ok(LoadedPaletteTexture {
            source_path: mapping.path,
            kind: mapping.kind,
            image: image.first_frame(),
        })
    }

    pub fn load_block_face(
        &self,
        block_state: &str,
        face: BlockFace,
    ) -> Result<RgbaTexture, AssetError> {
        let path = block_textures(block_state)
            .ok_or_else(|| {
                AssetError::MissingEntry(format!("block mapping for {}", block_id(block_state)))
            })?
            .for_face(face);
        Ok(self.load_png(path)?.first_frame())
    }
}

pub fn decode_png(bytes: &[u8]) -> Result<RgbaTexture, AssetError> {
    let image = image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|error| AssetError::Decode(error.to_string()))?
        .into_rgba8();
    Ok(RgbaTexture {
        width: image.width(),
        height: image.height(),
        pixels: image.into_raw(),
    })
}

/// Maps the current RedForge palette states to the closest Minecraft inventory
/// visual. `BlockTexture` entries are blocks whose inventory icon is generated
/// from a JSON model rather than stored as a ready-made PNG.
pub fn palette_texture(block_state: &str) -> Option<PaletteTexture> {
    use PaletteIconKind::{BlockTexture, ItemSprite};

    let (path, kind) = match block_id(block_state) {
        "minecraft:stone" => ("assets/minecraft/textures/block/stone.png", BlockTexture),
        "minecraft:redstone_wire" => ("assets/minecraft/textures/item/redstone.png", ItemSprite),
        "minecraft:redstone_torch" | "minecraft:redstone_wall_torch" => (
            "assets/minecraft/textures/block/redstone_torch.png",
            BlockTexture,
        ),
        "minecraft:repeater" => ("assets/minecraft/textures/item/repeater.png", ItemSprite),
        "minecraft:comparator" => ("assets/minecraft/textures/item/comparator.png", ItemSprite),
        "minecraft:redstone_lamp" => (
            "assets/minecraft/textures/block/redstone_lamp.png",
            BlockTexture,
        ),
        "minecraft:redstone_block" => (
            "assets/minecraft/textures/block/redstone_block.png",
            BlockTexture,
        ),
        "minecraft:lever" => ("assets/minecraft/textures/block/lever.png", BlockTexture),
        "minecraft:stone_button" => ("assets/minecraft/textures/block/stone.png", BlockTexture),
        "minecraft:observer" => (
            "assets/minecraft/textures/block/observer_front.png",
            BlockTexture,
        ),
        "minecraft:piston" => (
            "assets/minecraft/textures/block/piston_top.png",
            BlockTexture,
        ),
        "minecraft:sticky_piston" => (
            "assets/minecraft/textures/block/piston_top_sticky.png",
            BlockTexture,
        ),
        "minecraft:hopper" => ("assets/minecraft/textures/item/hopper.png", ItemSprite),
        "minecraft:barrel" => (
            "assets/minecraft/textures/block/barrel_side.png",
            BlockTexture,
        ),
        "minecraft:dispenser" => (
            "assets/minecraft/textures/block/dispenser_front.png",
            BlockTexture,
        ),
        "minecraft:dropper" => (
            "assets/minecraft/textures/block/dropper_front.png",
            BlockTexture,
        ),
        "minecraft:water" => (
            "assets/minecraft/textures/item/water_bucket.png",
            ItemSprite,
        ),
        "minecraft:oak_pressure_plate" => (
            "assets/minecraft/textures/block/oak_planks.png",
            BlockTexture,
        ),
        "minecraft:stone_pressure_plate" => {
            ("assets/minecraft/textures/block/stone.png", BlockTexture)
        }
        "minecraft:light_weighted_pressure_plate" => (
            "assets/minecraft/textures/block/gold_block.png",
            BlockTexture,
        ),
        "minecraft:heavy_weighted_pressure_plate" => (
            "assets/minecraft/textures/block/iron_block.png",
            BlockTexture,
        ),
        "minecraft:farmland" => (
            "assets/minecraft/textures/block/farmland_moist.png",
            BlockTexture,
        ),
        "minecraft:wheat" => ("assets/minecraft/textures/item/wheat.png", ItemSprite),
        "minecraft:brewing_stand" => (
            "assets/minecraft/textures/item/brewing_stand.png",
            ItemSprite,
        ),
        _ => return None,
    };
    Some(PaletteTexture { path, kind })
}

/// Representative model-local face textures for the current palette. Complex
/// shapes still need their JSON model geometry; this supplies their real pixels
/// instead of a synthetic color.
pub fn block_textures(block_state: &str) -> Option<BlockTextures> {
    let texture = match block_id(block_state) {
        "minecraft:stone" | "minecraft:stone_button" | "minecraft:stone_pressure_plate" => {
            BlockTextures::cube(concat!("assets/minecraft/textures/block/", "stone.png"))
        }
        "minecraft:redstone_wire" => BlockTextures::cube(concat!(
            "assets/minecraft/textures/block/",
            "redstone_dust_dot.png"
        )),
        "minecraft:redstone_torch" | "minecraft:redstone_wall_torch" => BlockTextures::cube(
            concat!("assets/minecraft/textures/block/", "redstone_torch.png"),
        ),
        "minecraft:repeater" => {
            let top = if block_state.contains("powered=true") {
                concat!("assets/minecraft/textures/block/", "repeater_on.png")
            } else {
                concat!("assets/minecraft/textures/block/", "repeater.png")
            };
            BlockTextures {
                top,
                bottom: concat!("assets/minecraft/textures/block/", "stone.png"),
                side: concat!("assets/minecraft/textures/block/", "stone.png"),
                front: top,
                back: top,
            }
        }
        "minecraft:comparator" => {
            let top = if block_state.contains("powered=true") {
                concat!("assets/minecraft/textures/block/", "comparator_on.png")
            } else {
                concat!("assets/minecraft/textures/block/", "comparator.png")
            };
            BlockTextures {
                top,
                bottom: concat!("assets/minecraft/textures/block/", "stone.png"),
                side: concat!("assets/minecraft/textures/block/", "stone.png"),
                front: top,
                back: top,
            }
        }
        "minecraft:redstone_lamp" => {
            let path = if block_state.contains("lit=true") {
                concat!("assets/minecraft/textures/block/", "redstone_lamp_on.png")
            } else {
                concat!("assets/minecraft/textures/block/", "redstone_lamp.png")
            };
            BlockTextures::cube(path)
        }
        "minecraft:redstone_block" => BlockTextures::cube(concat!(
            "assets/minecraft/textures/block/",
            "redstone_block.png"
        )),
        "minecraft:lever" => {
            BlockTextures::cube(concat!("assets/minecraft/textures/block/", "lever.png"))
        }
        "minecraft:observer" => BlockTextures {
            top: concat!("assets/minecraft/textures/block/", "observer_top.png"),
            bottom: concat!("assets/minecraft/textures/block/", "observer_top.png"),
            side: concat!("assets/minecraft/textures/block/", "observer_side.png"),
            front: concat!("assets/minecraft/textures/block/", "observer_front.png"),
            back: if block_state.contains("powered=true") {
                concat!("assets/minecraft/textures/block/", "observer_back_on.png")
            } else {
                concat!("assets/minecraft/textures/block/", "observer_back.png")
            },
        },
        "minecraft:piston" | "minecraft:sticky_piston" => BlockTextures {
            top: if block_id(block_state) == "minecraft:sticky_piston" {
                concat!("assets/minecraft/textures/block/", "piston_top_sticky.png")
            } else {
                concat!("assets/minecraft/textures/block/", "piston_top.png")
            },
            bottom: concat!("assets/minecraft/textures/block/", "piston_bottom.png"),
            side: concat!("assets/minecraft/textures/block/", "piston_side.png"),
            front: if block_id(block_state) == "minecraft:sticky_piston" {
                concat!("assets/minecraft/textures/block/", "piston_top_sticky.png")
            } else {
                concat!("assets/minecraft/textures/block/", "piston_top.png")
            },
            back: concat!("assets/minecraft/textures/block/", "piston_bottom.png"),
        },
        "minecraft:hopper" => BlockTextures {
            top: concat!("assets/minecraft/textures/block/", "hopper_inside.png"),
            bottom: concat!("assets/minecraft/textures/block/", "hopper_outside.png"),
            side: concat!("assets/minecraft/textures/block/", "hopper_outside.png"),
            front: concat!("assets/minecraft/textures/block/", "hopper_outside.png"),
            back: concat!("assets/minecraft/textures/block/", "hopper_outside.png"),
        },
        "minecraft:barrel" => BlockTextures {
            top: concat!("assets/minecraft/textures/block/", "barrel_top.png"),
            bottom: concat!("assets/minecraft/textures/block/", "barrel_bottom.png"),
            side: concat!("assets/minecraft/textures/block/", "barrel_side.png"),
            front: concat!("assets/minecraft/textures/block/", "barrel_side.png"),
            back: concat!("assets/minecraft/textures/block/", "barrel_side.png"),
        },
        "minecraft:dispenser" | "minecraft:dropper" => {
            let front = if block_id(block_state) == "minecraft:dispenser" {
                concat!("assets/minecraft/textures/block/", "dispenser_front.png")
            } else {
                concat!("assets/minecraft/textures/block/", "dropper_front.png")
            };
            BlockTextures {
                top: concat!("assets/minecraft/textures/block/", "furnace_top.png"),
                bottom: concat!("assets/minecraft/textures/block/", "furnace_top.png"),
                side: concat!("assets/minecraft/textures/block/", "furnace_side.png"),
                front,
                back: concat!("assets/minecraft/textures/block/", "furnace_side.png"),
            }
        }
        "minecraft:water" => BlockTextures::cube(concat!(
            "assets/minecraft/textures/block/",
            "water_still.png"
        )),
        "minecraft:oak_pressure_plate" => BlockTextures::cube(concat!(
            "assets/minecraft/textures/block/",
            "oak_planks.png"
        )),
        "minecraft:light_weighted_pressure_plate" => BlockTextures::cube(concat!(
            "assets/minecraft/textures/block/",
            "gold_block.png"
        )),
        "minecraft:heavy_weighted_pressure_plate" => BlockTextures::cube(concat!(
            "assets/minecraft/textures/block/",
            "iron_block.png"
        )),
        "minecraft:farmland" => BlockTextures {
            top: if block_state.contains("moisture=0") {
                concat!("assets/minecraft/textures/block/", "farmland.png")
            } else {
                concat!("assets/minecraft/textures/block/", "farmland_moist.png")
            },
            bottom: concat!("assets/minecraft/textures/block/", "dirt.png"),
            side: concat!("assets/minecraft/textures/block/", "dirt.png"),
            front: concat!("assets/minecraft/textures/block/", "dirt.png"),
            back: concat!("assets/minecraft/textures/block/", "dirt.png"),
        },
        "minecraft:wheat" => BlockTextures::cube(concat!(
            "assets/minecraft/textures/block/",
            "wheat_stage7.png"
        )),
        "minecraft:brewing_stand" => BlockTextures {
            top: concat!("assets/minecraft/textures/block/", "brewing_stand_base.png"),
            bottom: concat!("assets/minecraft/textures/block/", "brewing_stand_base.png"),
            side: concat!("assets/minecraft/textures/block/", "brewing_stand.png"),
            front: concat!("assets/minecraft/textures/block/", "brewing_stand.png"),
            back: concat!("assets/minecraft/textures/block/", "brewing_stand.png"),
        },
        _ => return None,
    };
    Some(texture)
}

fn block_id(block_state: &str) -> &str {
    block_state
        .split_once('[')
        .map_or(block_state, |(id, _)| id)
}

fn validate_resource_path(path: &str) -> Result<(), AssetError> {
    let safe = path.starts_with("assets/minecraft/")
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    if safe {
        Ok(())
    } else {
        Err(AssetError::UnsafePath(path.to_owned()))
    }
}

#[derive(Debug)]
struct Candidate {
    path: PathBuf,
    version: String,
    modified: SystemTime,
}

fn discover_candidates() -> Vec<Candidate> {
    let mut paths = BTreeSet::new();
    for root in launcher_roots() {
        collect_official_versions(&root.join("versions"), &mut paths);
        collect_library_clients(&root.join("libraries/com/mojang/minecraft"), &mut paths);
    }

    paths
        .into_iter()
        .filter_map(|path| {
            let modified = fs::metadata(&path)
                .ok()?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some(Candidate {
                version: version_hint(&path),
                path,
                modified,
            })
        })
        .collect()
}

fn launcher_roots() -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    if let Some(app_data) = env::var_os("APPDATA").map(PathBuf::from) {
        roots.insert(app_data.join(".minecraft"));
        roots.insert(app_data.join("PrismLauncher"));
        roots.insert(app_data.join("MultiMC"));
    }
    if let Some(local_data) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        roots.insert(local_data.join("PrismLauncher"));
        roots.insert(local_data.join("MultiMC"));
    }
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        roots.insert(home.join(".minecraft"));
        roots.insert(home.join(".local/share/PrismLauncher"));
        roots.insert(home.join(".local/share/multimc"));
        roots.insert(home.join("Library/Application Support/minecraft"));
        roots.insert(home.join("Library/Application Support/PrismLauncher"));
    }
    roots.into_iter().collect()
}

fn collect_official_versions(root: &Path, output: &mut BTreeSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let version = entry.file_name();
        let jar = entry.path().join(Path::new(&version).with_extension("jar"));
        if jar.is_file() {
            output.insert(jar);
        }
    }
}

fn collect_library_clients(root: &Path, output: &mut BTreeSet<PathBuf>) {
    let Ok(versions) = fs::read_dir(root) else {
        return;
    };
    for version in versions.flatten() {
        let Ok(files) = fs::read_dir(version.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name();
            let name = name.to_string_lossy();
            if file.path().is_file() && name.ends_with("-client.jar") {
                output.insert(file.path());
            }
        }
    }
}

fn version_hint(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown");
    if let Some(version) = stem
        .strip_prefix("minecraft-")
        .and_then(|value| value.strip_suffix("-client"))
    {
        version.to_owned()
    } else if stem == "client" {
        path.parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .unwrap_or(stem)
            .to_owned()
    } else {
        stem.to_owned()
    }
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    numeric_version_key(left)
        .cmp(&numeric_version_key(right))
        .then_with(|| left.cmp(right))
}

fn numeric_version_key(version: &str) -> Vec<u64> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_properties_before_mapping() {
        let mapping = palette_texture("minecraft:repeater[delay=2,facing=east,powered=false]")
            .expect("mapped repeater");
        assert_eq!(mapping.path, "assets/minecraft/textures/item/repeater.png");
        assert_eq!(mapping.kind, PaletteIconKind::ItemSprite);
    }

    #[test]
    fn dynamic_state_selects_lit_texture() {
        let lit = block_textures("minecraft:redstone_lamp[lit=true]").expect("lamp mapping");
        let unlit = block_textures("minecraft:redstone_lamp[lit=false]").expect("lamp mapping");
        assert!(lit.top.ends_with("redstone_lamp_on.png"));
        assert!(unlit.top.ends_with("redstone_lamp.png"));
    }

    #[test]
    fn animated_strip_is_cropped_to_first_frame() {
        let pixels = (0..32 * 4).map(|value| value as u8).collect();
        let frame = RgbaTexture {
            width: 2,
            height: 4,
            pixels,
        }
        .first_frame();
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(frame.pixels.len(), 16);
    }

    #[test]
    fn newer_year_version_sorts_after_legacy_version() {
        assert_eq!(compare_versions("26.2", "1.21.11"), Ordering::Greater);
        assert_eq!(compare_versions("1.21.11", "1.21.9"), Ordering::Greater);
    }

    #[test]
    fn rejects_archive_traversal() {
        assert!(validate_resource_path("assets/minecraft/textures/item/redstone.png").is_ok());
        assert!(validate_resource_path("assets/minecraft/../pack.mcmeta").is_err());
        assert!(validate_resource_path("/assets/minecraft/pack.mcmeta").is_err());
    }

    #[test]
    fn extracts_prism_version_name() {
        let path = Path::new("libraries/com/mojang/minecraft/26.2/minecraft-26.2-client.jar");
        assert_eq!(version_hint(path), "26.2");
    }

    #[test]
    fn installed_client_decodes_representative_textures_when_available() {
        let Ok(assets) = MinecraftAssets::discover() else {
            // CI and new installations need not have Minecraft installed.
            return;
        };

        let states = [
            "minecraft:stone",
            "minecraft:redstone_wire",
            "minecraft:redstone_torch[lit=true]",
            "minecraft:redstone_wall_torch[facing=north,lit=true]",
            "minecraft:repeater[powered=false]",
            "minecraft:comparator[powered=false]",
            "minecraft:redstone_lamp[lit=false]",
            "minecraft:redstone_block",
            "minecraft:lever",
            "minecraft:stone_button",
            "minecraft:observer[powered=false]",
            "minecraft:piston",
            "minecraft:sticky_piston",
            "minecraft:hopper",
            "minecraft:barrel",
            "minecraft:dispenser",
            "minecraft:dropper",
            "minecraft:water[level=0]",
            "minecraft:oak_pressure_plate",
            "minecraft:stone_pressure_plate",
            "minecraft:light_weighted_pressure_plate",
            "minecraft:heavy_weighted_pressure_plate",
            "minecraft:farmland[moisture=7]",
            "minecraft:wheat[age=7]",
            "minecraft:brewing_stand",
        ];
        for state in states {
            let texture = assets
                .load_palette_texture(state)
                .unwrap_or_else(|error| panic!("failed to load {state}: {error}"));
            assert!(texture.image.width > 0);
            assert!(texture.image.height > 0);
            assert_eq!(
                texture.image.pixels.len(),
                (texture.image.width * texture.image.height * 4) as usize
            );

            for face in [
                BlockFace::Top,
                BlockFace::Bottom,
                BlockFace::Side,
                BlockFace::Front,
                BlockFace::Back,
            ] {
                assets
                    .load_block_face(state, face)
                    .unwrap_or_else(|error| panic!("failed to load {state} {face:?}: {error}"));
            }
        }
    }
}
