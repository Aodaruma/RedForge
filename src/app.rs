use std::{
    collections::{BTreeMap, BTreeSet},
    f32::consts::FRAC_PI_2,
    path::PathBuf,
    sync::Arc,
};

use bevy::{
    app::AppExit,
    camera::ScalingMode,
    core_pipeline::tonemapping::Tonemapping,
    image::ImageSampler,
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    prelude::*,
    window::{PrimaryWindow, WindowCloseRequested, WindowResolution},
};
use bevy_egui::{
    EguiContexts, EguiPlugin, EguiPrimaryContextPass, EguiTextureHandle, egui,
    input::{egui_wants_any_keyboard_input, egui_wants_any_pointer_input},
};

use crate::{
    APP_NAME,
    brewing::{BrewEvent, BrewingStand},
    document::{BlockPos, Document, InventoryItem, rotate_state_y},
    minecraft_assets::{BlockFace, MinecraftAssets, block_textures},
    native_menu::{MenuAction, NativeMenuPlugin},
    simulation::{RedstoneSimulation, SettleMode},
};

const AIR: &str = "minecraft:air";

#[derive(Clone, Copy)]
struct PaletteItem {
    label: &'static str,
    state: &'static str,
}

const PALETTE: &[PaletteItem] = &[
    PaletteItem {
        label: "Stone",
        state: "minecraft:stone",
    },
    PaletteItem {
        label: "Redstone Dust",
        state: "minecraft:redstone_wire[east=none,north=none,power=0,south=none,west=none]",
    },
    PaletteItem {
        label: "Redstone Torch",
        state: "minecraft:redstone_torch[lit=true]",
    },
    PaletteItem {
        label: "Wall Torch",
        state: "minecraft:redstone_wall_torch[facing=north,lit=true]",
    },
    PaletteItem {
        label: "Repeater",
        state: "minecraft:repeater[delay=1,facing=north,locked=false,powered=false]",
    },
    PaletteItem {
        label: "Comparator",
        state: "minecraft:comparator[facing=north,mode=compare,powered=false]",
    },
    PaletteItem {
        label: "Redstone Lamp",
        state: "minecraft:redstone_lamp[lit=false]",
    },
    PaletteItem {
        label: "Redstone Block",
        state: "minecraft:redstone_block",
    },
    PaletteItem {
        label: "Lever",
        state: "minecraft:lever[face=floor,facing=north,powered=false]",
    },
    PaletteItem {
        label: "Stone Button",
        state: "minecraft:stone_button[face=floor,facing=north,powered=false]",
    },
    PaletteItem {
        label: "Observer",
        state: "minecraft:observer[facing=north,powered=false]",
    },
    PaletteItem {
        label: "Piston",
        state: "minecraft:piston[extended=false,facing=north]",
    },
    PaletteItem {
        label: "Sticky Piston",
        state: "minecraft:sticky_piston[extended=false,facing=north]",
    },
    PaletteItem {
        label: "Hopper",
        state: "minecraft:hopper[enabled=true,facing=down]",
    },
    PaletteItem {
        label: "Barrel",
        state: "minecraft:barrel[facing=north,open=false]",
    },
    PaletteItem {
        label: "Dispenser",
        state: "minecraft:dispenser[facing=north,triggered=false]",
    },
    PaletteItem {
        label: "Dropper",
        state: "minecraft:dropper[facing=north,triggered=false]",
    },
    PaletteItem {
        label: "Water Source",
        state: "minecraft:water[level=0]",
    },
    PaletteItem {
        label: "Oak Pressure Plate",
        state: "minecraft:oak_pressure_plate[powered=false]",
    },
    PaletteItem {
        label: "Stone Pressure Plate",
        state: "minecraft:stone_pressure_plate[powered=false]",
    },
    PaletteItem {
        label: "Light Weighted Plate",
        state: "minecraft:light_weighted_pressure_plate[power=0]",
    },
    PaletteItem {
        label: "Heavy Weighted Plate",
        state: "minecraft:heavy_weighted_pressure_plate[power=0]",
    },
    PaletteItem {
        label: "Farmland (static)",
        state: "minecraft:farmland[moisture=7]",
    },
    PaletteItem {
        label: "Wheat (static)",
        state: "minecraft:wheat[age=7]",
    },
    PaletteItem {
        label: "Brewing Stand",
        state: "minecraft:brewing_stand[has_bottle_0=false,has_bottle_1=false,has_bottle_2=false]",
    },
];

#[derive(Resource)]
struct Editor {
    document: Document,
    simulation: Option<RedstoneSimulation>,
    selected: Option<BlockPos>,
    selection: Option<(BlockPos, BlockPos)>,
    selection_anchor: Option<BlockPos>,
    clipboard: Vec<(BlockPos, String)>,
    inventory_for: Option<BlockPos>,
    inventory_draft: Vec<InventoryItem>,
    brewing: BTreeMap<BlockPos, BrewingStand>,
    brewing_log: Vec<String>,
    active_layer: i32,
    paint_state: String,
    palette_index: usize,
    tool: Tool,
    camera: CameraPreset,
    settle_mode: SettleMode,
    running: bool,
    ticks_per_second: f32,
    tick_accumulator: f32,
    probe_history: Vec<(u32, String)>,
    message: String,
    scene_epoch: u64,
    confirm_discard: bool,
    pending_file_action: Option<FileAction>,
    confirm_close: bool,
}

impl Editor {
    fn replace_document(&mut self, document: Document) {
        self.active_layer = document.minimum().y;
        self.document = document;
        self.simulation = None;
        self.running = false;
        self.probe_history.clear();
        self.selected = None;
        self.selection = None;
        self.selection_anchor = None;
        self.inventory_for = None;
        self.inventory_draft.clear();
        self.brewing.clear();
        self.brewing_log.clear();
        self.scene_epoch = self.scene_epoch.wrapping_add(1);
        self.confirm_discard = false;
        self.pending_file_action = None;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileAction {
    New,
    Open,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CameraPreset {
    Top,
    #[default]
    Isometric,
    Orbit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Tool {
    #[default]
    Paint,
    Select,
}

#[derive(Component)]
struct SceneVisual;

#[derive(Component)]
struct MainCamera;

#[derive(Component)]
struct CursorPreview;

#[derive(Component)]
struct SelectionPreview;

type SelectionPreviewQuery<'w, 's> = Single<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility),
    (With<SelectionPreview>, Without<CursorPreview>),
>;

#[derive(Resource, Default)]
struct RenderedRevision(Option<(u64, u64, Option<u32>, Option<BlockPos>)>);

#[derive(Resource, Default)]
struct CursorCell(Option<BlockPos>);

#[derive(Resource, Default)]
struct ViewportBounds(Option<egui::Rect>);

#[derive(Resource, Default)]
struct PaintDrag {
    button: Option<MouseButton>,
    cells: BTreeSet<BlockPos>,
    last: Option<BlockPos>,
}

#[derive(Resource)]
struct CameraRig {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    radius: f32,
}

#[derive(Default)]
struct AutosaveState {
    revision: u64,
    quiet_seconds: f32,
}

#[derive(Resource, Default)]
struct MinecraftVisuals {
    palette_icons: Vec<Option<Handle<Image>>>,
    textures: BTreeMap<&'static str, Handle<Image>>,
    source: String,
}

impl MinecraftVisuals {
    fn texture_for(&self, state: &str) -> Option<Handle<Image>> {
        let path = block_textures(state)?.for_face(BlockFace::Top);
        self.textures.get(path).cloned()
    }
}

pub fn run() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.035, 0.045, 0.06)))
        .insert_resource(Editor {
            document: Document::gate_zero_fixture().expect("built-in fixture must be valid"),
            simulation: None,
            selected: Some(BlockPos::new(0, 1, 0)),
            selection: None,
            selection_anchor: None,
            clipboard: Vec::new(),
            inventory_for: None,
            inventory_draft: Vec::new(),
            brewing: BTreeMap::new(),
            brewing_log: Vec::new(),
            active_layer: 1,
            paint_state: PALETTE[1].state.to_owned(),
            palette_index: 1,
            tool: Tool::Paint,
            camera: CameraPreset::Isometric,
            settle_mode: SettleMode::InWorld,
            running: false,
            ticks_per_second: 20.0,
            tick_accumulator: 0.0,
            probe_history: Vec::new(),
            message: "Gate 0 fixtureを読み込みました".to_owned(),
            scene_epoch: 0,
            confirm_discard: false,
            pending_file_action: None,
            confirm_close: false,
        })
        .insert_resource(CameraRig {
            target: Vec3::new(1.0, 0.5, 0.0),
            yaw: 0.7,
            pitch: -0.62,
            radius: 14.0,
        })
        .init_resource::<RenderedRevision>()
        .init_resource::<CursorCell>()
        .init_resource::<ViewportBounds>()
        .init_resource::<PaintDrag>()
        .init_resource::<MinecraftVisuals>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("{APP_NAME} — Technical Preview"),
                resolution: WindowResolution::new(1280, 800),
                ..default()
            }),
            close_when_requested: false,
            ..default()
        }))
        .add_plugins((EguiPlugin::default(), NativeMenuPlugin))
        .add_systems(Startup, (load_minecraft_visuals, setup).chain())
        .add_systems(Update, sync_scene)
        .add_systems(
            Update,
            (handle_menu_actions, file_shortcuts, update_window_title),
        )
        .add_systems(
            Update,
            (camera_keys, editor_keys).run_if(not(egui_wants_any_keyboard_input)),
        )
        .add_systems(
            Update,
            (camera_pointer, update_cursor, paint_input)
                .chain()
                .run_if(not(egui_wants_any_pointer_input)),
        )
        .add_systems(Update, clear_cursor.run_if(egui_wants_any_pointer_input))
        .add_systems(Update, (run_simulation, autosave, close_request))
        .add_systems(EguiPrimaryContextPass, editor_ui)
        .run();
}

fn load_minecraft_visuals(
    mut visuals: ResMut<MinecraftVisuals>,
    mut images: ResMut<Assets<Image>>,
) {
    let source = match MinecraftAssets::discover() {
        Ok(source) => source,
        Err(error) => {
            visuals.source = format!("Minecraft textures unavailable: {error}");
            visuals.palette_icons.resize(PALETTE.len(), None);
            return;
        }
    };

    visuals.source = format!("Minecraft {} · local assets", source.version());
    for item in PALETTE {
        let handle = source.load_palette_texture(item.state).ok().map(|texture| {
            if let Some(handle) = visuals.textures.get(texture.source_path) {
                return handle.clone();
            }
            let mut image = texture.image.into_bevy_image();
            image.sampler = ImageSampler::nearest();
            let handle = images.add(image);
            visuals.textures.insert(texture.source_path, handle.clone());
            handle
        });
        visuals.palette_icons.push(handle);
    }

    let dynamic_states = [
        "minecraft:redstone_lamp[lit=true]",
        "minecraft:repeater[powered=true]",
        "minecraft:comparator[powered=true]",
        "minecraft:observer[powered=true]",
        "minecraft:farmland[moisture=0]",
    ];
    for state in PALETTE.iter().map(|item| item.state).chain(dynamic_states) {
        let Some(textures) = block_textures(state) else {
            continue;
        };
        for face in [
            BlockFace::Top,
            BlockFace::Bottom,
            BlockFace::Side,
            BlockFace::Front,
            BlockFace::Back,
        ] {
            let path = textures.for_face(face);
            if visuals.textures.contains_key(path) {
                continue;
            }
            if let Ok(texture) = source.load_png(path) {
                let mut image = texture.first_frame().into_bevy_image();
                image.sampler = ImageSampler::nearest();
                visuals.textures.insert(path, images.add(image));
            }
        }
    }
    let grass = "assets/minecraft/textures/block/grass_block_top.png";
    if !visuals.textures.contains_key(grass)
        && let Ok(texture) = source.load_png(grass)
    {
        let mut image = texture.into_bevy_image();
        image.sampler = ImageSampler::nearest();
        visuals.textures.insert(grass, images.add(image));
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    visuals: Res<MinecraftVisuals>,
) {
    commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 18.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(8.0, 9.0, 10.0).looking_at(Vec3::new(1.0, 0.0, 0.0), Vec3::Y),
        MainCamera,
    ));
    let ground_material = materials.add(StandardMaterial {
        base_color: if visuals
            .textures
            .contains_key("assets/minecraft/textures/block/grass_block_top.png")
        {
            // Minecraft's grass texture expects a biome tint in the renderer.
            Color::srgb(0.48, 0.75, 0.35)
        } else {
            Color::srgb(0.22, 0.34, 0.19)
        },
        base_color_texture: visuals
            .textures
            .get("assets/minecraft/textures/block/grass_block_top.png")
            .cloned(),
        unlit: true,
        ..default()
    });
    let ground_tile = meshes.add(Cuboid::new(1.0, 0.04, 1.0));
    for x in -16..16 {
        for z in -16..16 {
            commands.spawn((
                Mesh3d(ground_tile.clone()),
                MeshMaterial3d(ground_material.clone()),
                Transform::from_xyz(x as f32, 0.475, z as f32),
            ));
        }
    }
    let minor_grid = materials.add(StandardMaterial {
        base_color: Color::srgba(0.055, 0.065, 0.075, 0.72),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let major_grid = materials.add(StandardMaterial {
        base_color: Color::srgba(0.42, 0.66, 0.75, 0.88),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let x_line = meshes.add(Cuboid::new(32.0, 0.012, 0.018));
    let z_line = meshes.add(Cuboid::new(0.018, 0.012, 32.0));
    for index in -16..=16 {
        let coordinate = index as f32 - 0.5;
        let material = if index % 4 == 0 {
            major_grid.clone()
        } else {
            minor_grid.clone()
        };
        commands.spawn((
            Mesh3d(x_line.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(-0.5, 0.502, coordinate),
        ));
        commands.spawn((
            Mesh3d(z_line.clone()),
            MeshMaterial3d(material),
            Transform::from_xyz(coordinate, 0.502, -0.5),
        ));
    }
    let preview_mesh = meshes.add(Cuboid::new(0.98, 0.98, 0.98));
    commands.spawn((
        Mesh3d(preview_mesh.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.2, 0.85, 1.0, 0.3),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
        Visibility::Hidden,
        CursorPreview,
    ));
    commands.spawn((
        Mesh3d(preview_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.7, 0.1, 0.18),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
        Visibility::Hidden,
        SelectionPreview,
    ));
}

fn sync_scene(
    mut commands: Commands,
    editor: Res<Editor>,
    minecraft_visuals: Res<MinecraftVisuals>,
    mut rendered: ResMut<RenderedRevision>,
    scene_visuals: Query<Entity, With<SceneVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let signature = (
        editor.scene_epoch,
        editor.document.revision,
        editor.simulation.as_ref().map(RedstoneSimulation::tick),
        editor.selected,
    );
    if rendered.0 == Some(signature) {
        return;
    }
    for entity in &scene_visuals {
        commands.entity(entity).despawn();
    }
    let mut positions: BTreeSet<_> = editor
        .document
        .blocks()
        .into_iter()
        .map(|(pos, _)| pos)
        .collect();
    if let Some(simulation) = editor.simulation.as_ref()
        && let Ok(changes) = simulation.changes()
    {
        positions.extend(
            changes
                .into_iter()
                .map(|change| BlockPos::new(change.pos[0], change.pos[1], change.pos[2])),
        );
    }
    for pos in positions {
        let state = if editor.brewing.contains_key(&pos) {
            editor.document.block(pos)
        } else {
            editor
                .simulation
                .as_ref()
                .map(|simulation| simulation.block(pos))
                .unwrap_or_else(|| editor.document.block(pos))
        };
        if state == AIR {
            continue;
        }
        let (size, offset) = block_shape(&state);
        let selected = editor.selected == Some(pos);
        let color = if let Some(stand) = editor.brewing.get(&pos) {
            let progress = stand.progress();
            Color::srgb(
                0.45 - progress * 0.15,
                0.3 + progress * 0.35,
                0.42 + progress * 0.35,
            )
        } else {
            block_color(&state)
        };
        let texture = minecraft_visuals.texture_for(&state);
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: if selected && texture.is_some() {
                    Color::srgb(0.72, 0.98, 1.0)
                } else if texture.is_some() {
                    Color::WHITE
                } else if selected {
                    Color::srgb(0.12, 0.82, 0.92)
                } else {
                    color
                },
                base_color_texture: texture,
                alpha_mode: if state.starts_with("minecraft:water") {
                    AlphaMode::Blend
                } else if state.contains("wheat")
                    || state.contains("torch")
                    || state.contains("redstone_wire")
                    || state.contains("lever")
                {
                    AlphaMode::Mask(0.5)
                } else {
                    AlphaMode::Opaque
                },
                perceptual_roughness: 0.72,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(
                Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32) + offset,
            ),
            SceneVisual,
        ));
        if let Some((rotation, direction)) = facing_marker(&state) {
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.12, 0.1, 0.62))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.98, 0.82, 0.16),
                    unlit: true,
                    ..default()
                })),
                Transform {
                    translation: block_vec3(pos) + Vec3::Y * 0.53 + direction * 0.18,
                    rotation,
                    ..default()
                },
                SceneVisual,
            ));
        }
        if let Some(power) =
            state_property(&state, "power").and_then(|value| value.parse::<u8>().ok())
        {
            let height = 0.1 + power as f32 / 15.0 * 0.65;
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.12, height, 0.12))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(1.0, 0.25, 0.05),
                    unlit: true,
                    ..default()
                })),
                Transform::from_translation(block_vec3(pos) + Vec3::new(0.34, height * 0.5, 0.34)),
                SceneVisual,
            ));
        }
        if let Some(stand) = editor.brewing.get(&pos) {
            let output = stand.comparator_output();
            let height = 0.08 + f32::from(output) / 15.0 * 0.62;
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.1, height, 0.1))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.78, 0.18, 0.92),
                    unlit: true,
                    ..default()
                })),
                Transform::from_translation(block_vec3(pos) + Vec3::new(-0.35, height * 0.5, 0.35)),
                SceneVisual,
            ));
        }
    }
    rendered.0 = Some(signature);
}

fn update_cursor(
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    window: Single<&Window>,
    editor: Res<Editor>,
    viewport_bounds: Res<ViewportBounds>,
    mut cursor: ResMut<CursorCell>,
    cursor_preview: Single<(&mut Transform, &mut Visibility), With<CursorPreview>>,
    selection_preview: SelectionPreviewQuery,
) {
    let in_viewport = window.cursor_position().is_some_and(|screen| {
        viewport_bounds
            .0
            .is_some_and(|bounds| bounds.contains(egui::pos2(screen.x, screen.y)))
    });
    cursor.0 = if editor.camera == CameraPreset::Orbit || !in_viewport {
        None
    } else {
        window
            .cursor_position()
            .and_then(|screen| camera.0.viewport_to_world(camera.1, screen).ok())
            .and_then(|ray| {
                ray.plane_intersection_point(
                    Vec3::new(0.0, editor.active_layer as f32 + 0.5, 0.0),
                    InfinitePlane3d::new(Vec3::Y),
                )
            })
            .map(|point| {
                BlockPos::new(
                    (point.x + 0.5).floor() as i32,
                    editor.active_layer,
                    (point.z + 0.5).floor() as i32,
                )
            })
    };
    let (mut transform, mut visibility) = cursor_preview.into_inner();
    if let Some(pos) = cursor.0 {
        transform.translation = Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32);
        *visibility = Visibility::Visible;
    } else {
        *visibility = Visibility::Hidden;
    }

    let (mut transform, mut visibility) = selection_preview.into_inner();
    if let Some((a, b)) = editor.selection {
        let min = Vec3::new(
            a.x.min(b.x) as f32,
            a.y.min(b.y) as f32,
            a.z.min(b.z) as f32,
        );
        let max = Vec3::new(
            a.x.max(b.x) as f32,
            a.y.max(b.y) as f32,
            a.z.max(b.z) as f32,
        );
        transform.translation = (min + max) * 0.5;
        transform.scale = max - min + Vec3::ONE;
        *visibility = Visibility::Visible;
    } else {
        *visibility = Visibility::Hidden;
    }
}

fn paint_input(
    buttons: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorCell>,
    mut drag: ResMut<PaintDrag>,
    mut editor: ResMut<Editor>,
) {
    let Some(cell) = cursor.0 else {
        if buttons.just_released(MouseButton::Left) || buttons.just_released(MouseButton::Right) {
            finish_drag(&mut editor, &mut drag);
        }
        return;
    };
    if buttons.just_pressed(MouseButton::Left) && editor.tool == Tool::Select {
        if let Some(anchor) = editor.selection_anchor.take() {
            editor.selection = Some((anchor, cell));
            editor.message = format!("選択範囲: {anchor:?} → {cell:?}");
        } else {
            editor.selection_anchor = Some(cell);
            editor.selection = Some((cell, cell));
            editor.message = "選択範囲の終点をクリックしてください".to_owned();
        }
        editor.selected = Some(cell);
        return;
    }
    if buttons.just_pressed(MouseButton::Left) && editor.tool == Tool::Paint {
        begin_drag(&mut editor, &mut drag, MouseButton::Left, cell);
    }
    if buttons.just_pressed(MouseButton::Right) {
        begin_drag(&mut editor, &mut drag, MouseButton::Right, cell);
    }
    if let Some(button) = drag.button
        && buttons.pressed(button)
        && drag.last != Some(cell)
    {
        let previous = drag.last.unwrap_or(cell);
        add_line(&mut drag.cells, previous, cell);
        drag.last = Some(cell);
    }
    if buttons.just_released(MouseButton::Left) || buttons.just_released(MouseButton::Right) {
        finish_drag(&mut editor, &mut drag);
    }
}

fn clear_cursor(
    mut cursor: ResMut<CursorCell>,
    mut preview: Single<&mut Visibility, With<CursorPreview>>,
) {
    cursor.0 = None;
    **preview = Visibility::Hidden;
}

fn begin_drag(editor: &mut Editor, drag: &mut PaintDrag, button: MouseButton, cell: BlockPos) {
    if editor.simulation.take().is_some() {
        editor.message = "編集を開始したためsimulation snapshotを破棄しました".to_owned();
    }
    editor.brewing.clear();
    editor.brewing_log.clear();
    drag.button = Some(button);
    drag.cells.clear();
    drag.cells.insert(cell);
    drag.last = Some(cell);
    editor.selected = Some(cell);
}

fn finish_drag(editor: &mut Editor, drag: &mut PaintDrag) {
    let Some(button) = drag.button.take() else {
        return;
    };
    let state = if button == MouseButton::Right {
        AIR
    } else {
        &editor.paint_state
    };
    let cells = drag.cells.iter().copied().map(|pos| (pos, state));
    match editor.document.apply_cells(cells) {
        Ok(count) => editor.message = format!("{count} cellsを更新しました"),
        Err(error) => editor.message = error,
    }
    drag.cells.clear();
    drag.last = None;
}

fn add_line(cells: &mut BTreeSet<BlockPos>, from: BlockPos, to: BlockPos) {
    let dx = (to.x - from.x).abs();
    let dz = (to.z - from.z).abs();
    let steps = dx.max(dz).max(1);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        cells.insert(BlockPos::new(
            (from.x as f32 + (to.x - from.x) as f32 * t).round() as i32,
            to.y,
            (from.z as f32 + (to.z - from.z) as f32 * t).round() as i32,
        ));
    }
}

fn invalidate_simulation(editor: &mut Editor) {
    editor.simulation = None;
    editor.brewing.clear();
    editor.brewing_log.clear();
    editor.running = false;
}

fn undo_redo(editor: &mut Editor, redo: bool) {
    let result = if redo {
        editor.document.redo()
    } else {
        editor.document.undo()
    };
    if let Err(error) = result {
        editor.message = error;
    }
    invalidate_simulation(editor);
}

fn rotate_paint(editor: &mut Editor) {
    match rotate_state_y(&editor.paint_state) {
        Ok(state) => editor.paint_state = state,
        Err(error) => editor.message = error,
    }
}

fn copy_selection(editor: &mut Editor) {
    let Some((a, b)) = editor.selection else {
        editor.message = "選択範囲がありません".to_owned();
        return;
    };
    let origin = BlockPos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z));
    editor.clipboard = editor
        .document
        .selected_blocks(a, b)
        .into_iter()
        .map(|(pos, state)| {
            (
                BlockPos::new(pos.x - origin.x, pos.y - origin.y, pos.z - origin.z),
                state,
            )
        })
        .collect();
    editor.message = format!("{} blocksをcopyしました", editor.clipboard.len());
}

fn paste_clipboard(editor: &mut Editor, origin: Option<BlockPos>) {
    let Some(origin) = origin else {
        editor.message = "paste先のセルを指してください".to_owned();
        return;
    };
    let cells: Vec<_> = editor
        .clipboard
        .iter()
        .map(|(offset, state)| {
            (
                BlockPos::new(
                    origin.x + offset.x,
                    origin.y + offset.y,
                    origin.z + offset.z,
                ),
                state.clone(),
            )
        })
        .collect();
    match editor.document.apply_cells(cells) {
        Ok(count) => editor.message = format!("{count} blocksをpasteしました"),
        Err(error) => editor.message = error,
    }
}

fn delete_selection(editor: &mut Editor) {
    let Some((a, b)) = editor.selection else {
        editor.message = "選択範囲がありません".to_owned();
        return;
    };
    let cells: Vec<_> = editor
        .document
        .selected_blocks(a, b)
        .into_iter()
        .map(|(pos, _)| (pos, AIR))
        .collect();
    match editor.document.apply_cells(cells) {
        Ok(count) => editor.message = format!("{count} blocksを削除しました"),
        Err(error) => editor.message = error,
    }
}

fn focus_selection(editor: &Editor, rig: &mut CameraRig) {
    let focus = editor
        .selection
        .map(|(a, b)| {
            Vec3::new(
                (a.x + b.x) as f32 * 0.5,
                (a.y + b.y) as f32 * 0.5,
                (a.z + b.z) as f32 * 0.5,
            )
        })
        .or_else(|| editor.selected.map(block_vec3));
    if let Some(focus) = focus {
        rig.target = focus;
    }
}

fn use_selected(editor: &mut Editor) {
    let (Some(pos), Some(simulation)) = (editor.selected, editor.simulation.as_mut()) else {
        editor.message = "simulationと操作対象を選んでください".to_owned();
        return;
    };
    simulation.use_block(pos);
    editor.scene_epoch = editor.scene_epoch.wrapping_add(1);
    record_probe(editor);
}

fn toggle_simulation(editor: &mut Editor) {
    if editor.simulation.is_none() {
        start_simulation(editor);
    } else {
        editor.running = !editor.running;
        editor.message = if editor.running {
            "simulationを実行中".to_owned()
        } else {
            "simulationをpauseしました".to_owned()
        };
    }
}

fn reset_simulation(editor: &mut Editor) {
    invalidate_simulation(editor);
    editor.probe_history.clear();
    editor.scene_epoch = editor.scene_epoch.wrapping_add(1);
}

fn handle_menu_actions(
    mut actions: MessageReader<MenuAction>,
    cursor: Res<CursorCell>,
    mut editor: ResMut<Editor>,
    mut rig: ResMut<CameraRig>,
    mut exit: MessageWriter<AppExit>,
) {
    for action in actions.read().copied() {
        match action {
            MenuAction::New => request_file_action(&mut editor, FileAction::New),
            MenuAction::Open => request_file_action(&mut editor, FileAction::Open),
            MenuAction::Save => save_document(&mut editor),
            MenuAction::SaveAs => save_document_as(&mut editor),
            MenuAction::Exit if editor.document.dirty => editor.confirm_close = true,
            MenuAction::Exit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Undo => undo_redo(&mut editor, false),
            MenuAction::Redo => undo_redo(&mut editor, true),
            MenuAction::Copy => copy_selection(&mut editor),
            MenuAction::Paste => paste_clipboard(&mut editor, cursor.0),
            MenuAction::Delete => delete_selection(&mut editor),
            MenuAction::Rotate => rotate_paint(&mut editor),
            MenuAction::Paint => editor.tool = Tool::Paint,
            MenuAction::Select => editor.tool = Tool::Select,
            MenuAction::Top => editor.camera = CameraPreset::Top,
            MenuAction::Isometric => editor.camera = CameraPreset::Isometric,
            MenuAction::Orbit => editor.camera = CameraPreset::Orbit,
            MenuAction::LayerDown => editor.active_layer -= 1,
            MenuAction::LayerUp => editor.active_layer += 1,
            MenuAction::Focus => focus_selection(&editor, &mut rig),
            MenuAction::Start => start_simulation(&mut editor),
            MenuAction::Use => use_selected(&mut editor),
            MenuAction::Step => step_simulation(&mut editor, 1),
            MenuAction::RunPause => toggle_simulation(&mut editor),
            MenuAction::Reset => reset_simulation(&mut editor),
        }
    }
}

fn file_shortcuts(keys: Res<ButtonInput<KeyCode>>, mut editor: ResMut<Editor>) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ctrl {
        return;
    }
    if keys.just_pressed(KeyCode::KeyN) {
        request_file_action(&mut editor, FileAction::New);
    }
    if keys.just_pressed(KeyCode::KeyO) {
        request_file_action(&mut editor, FileAction::Open);
    }
    if keys.just_pressed(KeyCode::KeyS) {
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        if shift {
            save_document_as(&mut editor);
        } else {
            save_document(&mut editor);
        }
    }
}

fn editor_keys(
    keys: Res<ButtonInput<KeyCode>>,
    cursor: Res<CursorCell>,
    mut editor: ResMut<Editor>,
    mut rig: ResMut<CameraRig>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if keys.just_pressed(KeyCode::KeyV) && !ctrl {
        editor.tool = if editor.tool == Tool::Paint {
            Tool::Select
        } else {
            Tool::Paint
        };
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        editor.active_layer -= 1;
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        editor.active_layer += 1;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        rotate_paint(&mut editor);
    }
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        undo_redo(&mut editor, shift);
    }
    if ctrl && keys.just_pressed(KeyCode::KeyC) {
        copy_selection(&mut editor);
    }
    if ctrl && keys.just_pressed(KeyCode::KeyV) {
        paste_clipboard(&mut editor, cursor.0);
    }
    if keys.just_pressed(KeyCode::Delete) {
        delete_selection(&mut editor);
    }
    if keys.just_pressed(KeyCode::Escape) {
        editor.selection = None;
        editor.selection_anchor = None;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        focus_selection(&editor, &mut rig);
    }
    if keys.just_pressed(KeyCode::Space) {
        toggle_simulation(&mut editor);
    }
    if keys.just_pressed(KeyCode::Period) {
        step_simulation(&mut editor, 1);
    }
}

fn start_simulation(editor: &mut Editor) {
    match RedstoneSimulation::start(&editor.document, editor.settle_mode, 0) {
        Ok(simulation) => {
            editor.brewing = editor
                .document
                .blocks()
                .into_iter()
                .filter(|(_, state)| state.starts_with("minecraft:brewing_stand"))
                .map(|(pos, _)| {
                    let (brew_time, fuel) = editor.document.brewing_timers(pos);
                    (
                        pos,
                        BrewingStand::new(editor.document.inventory(pos), brew_time, fuel),
                    )
                })
                .collect();
            editor.brewing_log.clear();
            editor.message = simulation.summary();
            editor.simulation = Some(simulation);
            editor.running = false;
            editor.tick_accumulator = 0.0;
            editor.probe_history.clear();
            editor.scene_epoch = editor.scene_epoch.wrapping_add(1);
            record_probe(editor);
        }
        Err(error) => editor.message = error,
    }
}

fn step_simulation(editor: &mut Editor, ticks: u32) {
    if editor.simulation.is_none() {
        return;
    }
    for _ in 0..ticks {
        let tick = {
            let simulation = editor.simulation.as_mut().expect("checked above");
            simulation.step();
            simulation.tick()
        };
        for (pos, stand) in &mut editor.brewing {
            if let Some(event) = stand.tick() {
                editor.brewing_log.push(format!(
                    "t{tick} ({},{},{}): {}",
                    pos.x,
                    pos.y,
                    pos.z,
                    brew_event_label(event)
                ));
            }
        }
    }
    if editor.brewing_log.len() > 64 {
        editor.brewing_log.drain(..editor.brewing_log.len() - 64);
    }
    let tick = editor.simulation.as_ref().expect("checked above").tick();
    editor.message = format!("tick {tick}");
    editor.scene_epoch = editor.scene_epoch.wrapping_add(1);
    record_probe(editor);
}

fn record_probe(editor: &mut Editor) {
    let (Some(pos), Some(simulation)) = (editor.selected, editor.simulation.as_ref()) else {
        return;
    };
    editor
        .probe_history
        .push((simulation.tick(), simulation.block(pos)));
    if editor.probe_history.len() > 128 {
        editor.probe_history.remove(0);
    }
}

fn run_simulation(time: Res<Time>, mut editor: ResMut<Editor>) {
    if !editor.running || editor.simulation.is_none() {
        return;
    }
    editor.tick_accumulator += time.delta_secs() * editor.ticks_per_second;
    let ticks = editor.tick_accumulator.floor().min(20.0) as u32;
    if ticks == 0 {
        return;
    }
    editor.tick_accumulator -= ticks as f32;
    step_simulation(&mut editor, ticks);
}

fn camera_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: ResMut<Editor>,
    mut rig: ResMut<CameraRig>,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    if keys.just_pressed(KeyCode::Digit1) {
        editor.camera = CameraPreset::Top;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        editor.camera = CameraPreset::Isometric;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        editor.camera = CameraPreset::Orbit;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        rig.yaw -= FRAC_PI_2;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        rig.yaw += FRAC_PI_2;
    }
    let (mut transform, _) = camera.into_inner();
    apply_camera(&mut transform, editor.camera, &rig);
}

fn camera_pointer(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    editor: Res<Editor>,
    mut rig: ResMut<CameraRig>,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let (mut transform, mut projection) = camera.into_inner();
    if buttons.pressed(MouseButton::Middle) {
        if editor.camera == CameraPreset::Orbit {
            rig.yaw -= motion.delta.x * 0.005;
            rig.pitch = (rig.pitch - motion.delta.y * 0.005).clamp(-FRAC_PI_2 + 0.05, -0.05);
        } else {
            let factor = orthographic_scale(&projection) * 0.003;
            rig.target -= transform.right().as_vec3() * motion.delta.x * factor;
            rig.target += transform.up().as_vec3() * motion.delta.y * factor;
        }
    }
    for event in wheel.read() {
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scale = (orthographic.scale * (-event.y * 0.1).exp()).clamp(0.05, 20.0);
        }
    }
    apply_camera(&mut transform, editor.camera, &rig);
}

fn apply_camera(transform: &mut Transform, preset: CameraPreset, rig: &CameraRig) {
    *transform = match preset {
        CameraPreset::Top => Transform::from_translation(rig.target + Vec3::Y * rig.radius)
            .looking_at(rig.target, Vec3::NEG_Z),
        CameraPreset::Isometric => {
            let rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, -0.62, 0.0);
            Transform::from_translation(rig.target - rotation * Vec3::NEG_Z * rig.radius)
                .looking_at(rig.target, Vec3::Y)
        }
        CameraPreset::Orbit => {
            let rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0);
            Transform::from_translation(rig.target - rotation * Vec3::NEG_Z * rig.radius)
                .looking_at(rig.target, Vec3::Y)
        }
    };
}

fn orthographic_scale(projection: &Projection) -> f32 {
    match projection {
        Projection::Orthographic(value) => value.scale,
        _ => 1.0,
    }
}

fn close_request(
    mut requests: MessageReader<WindowCloseRequested>,
    mut editor: ResMut<Editor>,
    mut exit: MessageWriter<AppExit>,
) {
    if requests.read().next().is_some() {
        if editor.document.dirty {
            editor.confirm_close = true;
        } else {
            exit.write(AppExit::Success);
        }
    }
}

fn update_window_title(editor: Res<Editor>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    if !editor.is_changed() {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let name = editor
        .document
        .path()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled");
    window.title = format!(
        "{name}{} — {APP_NAME}",
        if editor.document.dirty { " *" } else { "" }
    );
}

fn autosave(time: Res<Time>, mut editor: ResMut<Editor>, mut state: Local<AutosaveState>) {
    if !editor.document.dirty {
        state.revision = editor.document.revision;
        state.quiet_seconds = 0.0;
        return;
    }
    if state.revision != editor.document.revision {
        state.revision = editor.document.revision;
        state.quiet_seconds = 0.0;
        return;
    }
    state.quiet_seconds += time.delta_secs();
    if state.quiet_seconds < 2.0 {
        return;
    }
    let path = recovery_path();
    match editor.document.write_copy(&path) {
        Ok(()) => editor.message = format!("recovery保存: {}", path.display()),
        Err(error) => editor.message = format!("recovery保存失敗: {error}"),
    }
    state.quiet_seconds = -3_600.0;
}

fn recovery_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("RedForge")
        .join("recovery.litematic")
}

fn editor_ui(
    mut contexts: EguiContexts,
    mut editor: ResMut<Editor>,
    visuals: Res<MinecraftVisuals>,
    mut viewport_bounds: ResMut<ViewportBounds>,
    mut exit: MessageWriter<AppExit>,
    mut fonts_configured: Local<bool>,
) -> Result {
    let palette_textures: Vec<_> = visuals
        .palette_icons
        .iter()
        .map(|handle| {
            handle
                .as_ref()
                .map(|handle| contexts.add_image(EguiTextureHandle::Strong(handle.clone())))
        })
        .collect();
    let ctx = contexts.ctx_mut()?;
    if !*fonts_configured {
        configure_fonts(ctx);
        *fonts_configured = true;
    }
    sync_inventory_editor(&mut editor);
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::left("palette")
        .resizable(false)
        .default_size(232.0)
        .frame(egui::Frame::side_top_panel(&ctx.style_of(egui::Theme::Dark)).inner_margin(12.0))
        .show(&mut root, |ui| {
            ui.heading("Palette");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut editor.tool, Tool::Paint, "Paint");
                ui.selectable_value(&mut editor.tool, Tool::Select, "Select (V)");
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("palette-icon-grid")
                    .num_columns(4)
                    .spacing(egui::vec2(6.0, 6.0))
                    .show(ui, |ui| {
                        for (index, item) in PALETTE.iter().enumerate() {
                            let selected = editor.palette_index == index;
                            let response = if let Some(texture) = palette_textures[index] {
                                let image = egui::Image::new((texture, egui::vec2(34.0, 34.0)))
                                    .texture_options(egui::TextureOptions::NEAREST);
                                ui.add(
                                    egui::Button::image(image)
                                        .selected(selected)
                                        .min_size(egui::vec2(46.0, 46.0)),
                                )
                            } else {
                                ui.add(
                                    egui::Button::new(short_palette_label(item.label))
                                        .selected(selected)
                                        .min_size(egui::vec2(46.0, 46.0)),
                                )
                            }
                            .on_hover_text(format!("{}\n{}", item.label, item.state));
                            if response.clicked() {
                                editor.palette_index = index;
                                editor.paint_state = item.state.to_owned();
                                editor.tool = Tool::Paint;
                            }
                            if (index + 1) % 4 == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
            ui.separator();
            ui.strong(PALETTE[editor.palette_index].label);
            ui.small(&visuals.source);
            ui.label("R: facingを90°回転");
            ui.small("左drag: 配置 / 右drag: 削除");
        });
    egui::Panel::right("inspector")
        .resizable(true)
        .default_size(320.0)
        .frame(egui::Frame::side_top_panel(&ctx.style_of(egui::Theme::Dark)).inner_margin(12.0))
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Inspector");
                if let Some(pos) = editor.selected {
                    ui.monospace(format!("x={} y={} z={}", pos.x, pos.y, pos.z));
                    let state = if editor.brewing.contains_key(&pos) {
                        editor.document.block(pos)
                    } else {
                        editor
                            .simulation
                            .as_ref()
                            .map(|simulation| simulation.block(pos))
                            .unwrap_or_else(|| editor.document.block(pos))
                    };
                    ui.monospace(state);
                    if editor.document.inventory_slot_count(pos).is_some() {
                        ui.collapsing("Inventory", |ui| {
                            if editor
                                .document
                                .block(pos)
                                .starts_with("minecraft:brewing_stand")
                            {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Preset:");
                                    if ui.button("水→奇妙").clicked() {
                                        brewing_preset(
                                            &mut editor.inventory_draft,
                                            "minecraft:water",
                                            "minecraft:nether_wart",
                                        );
                                    }
                                    if ui.button("奇妙→俊敏").clicked() {
                                        brewing_preset(
                                            &mut editor.inventory_draft,
                                            "minecraft:awkward",
                                            "minecraft:sugar",
                                        );
                                    }
                                    if ui.button("俊敏→延長").clicked() {
                                        brewing_preset(
                                            &mut editor.inventory_draft,
                                            "minecraft:swiftness",
                                            "minecraft:redstone",
                                        );
                                    }
                                });
                            }
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    for item in &mut editor.inventory_draft {
                                        ui.horizontal(|ui| {
                                            ui.label(format!("{}", item.slot));
                                            ui.add(
                                                egui::TextEdit::singleline(&mut item.id)
                                                    .desired_width(145.0),
                                            );
                                            ui.add(
                                                egui::DragValue::new(&mut item.count).range(0..=64),
                                            );
                                        });
                                        if let Some(potion) = item.potion() {
                                            ui.small(format!("  potion: {potion}"));
                                        }
                                    }
                                });
                            if ui.button("Inventoryを適用").clicked() {
                                let items: Vec<_> = editor
                                    .inventory_draft
                                    .iter()
                                    .filter(|item| !item.id.trim().is_empty() && item.count > 0)
                                    .cloned()
                                    .collect();
                                match editor.document.set_inventory(pos, items) {
                                    Ok(true) => {
                                        editor.message = "Inventoryを更新しました".to_owned();
                                        editor.simulation = None;
                                        editor.brewing.clear();
                                        editor.brewing_log.clear();
                                        editor.running = false;
                                        editor.inventory_for = None;
                                    }
                                    Ok(false) => {
                                        editor.message = "Inventoryは変更なしです".to_owned()
                                    }
                                    Err(error) => editor.message = error,
                                }
                            }
                        });
                    }
                    if let Some(stand) = editor.brewing.get(&pos) {
                        ui.separator();
                        ui.strong("Brewing (RedForge extension)");
                        ui.add(
                            egui::ProgressBar::new(stand.progress())
                                .text(format!("残り {} / 400 tick", stand.brew_time)),
                        );
                        ui.label(format!(
                            "燃料 {} / 比較器 {} / 瓶 {:?}",
                            stand.fuel,
                            stand.comparator_output(),
                            stand.bottle_flags()
                        ));
                        let contents: Vec<_> = stand
                            .items()
                            .iter()
                            .filter(|item| item.slot < 3)
                            .map(|item| item.potion().unwrap_or(&item.id))
                            .collect();
                        ui.small(format!("内容: {}", contents.join(", ")));
                        if stand.observer_pulse {
                            ui.colored_label(egui::Color32::YELLOW, "Observer pulse");
                        }
                        ui.small("比較器値は正確に計算。Nucleation下流への動的伝播は未対応です。");
                    }
                } else {
                    ui.label("セルを選択してください");
                }
                ui.separator();
                ui.label("配置state");
                ui.monospace(&editor.paint_state);
                ui.separator();
                ui.heading("Simulation");
                egui::ComboBox::from_id_salt("settle")
                    .selected_text(format!("{:?}", editor.settle_mode))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut editor.settle_mode,
                            SettleMode::InWorld,
                            "InWorld",
                        );
                        ui.selectable_value(
                            &mut editor.settle_mode,
                            SettleMode::Placement,
                            "Placement",
                        );
                        ui.selectable_value(&mut editor.settle_mode, SettleMode::Quiet, "Quiet");
                    });
                ui.horizontal(|ui| {
                    if ui.button("Start").clicked() {
                        start_simulation(&mut editor);
                    }
                    if ui.button("Use").clicked() {
                        use_selected(&mut editor);
                    }
                    if ui.button("Step").clicked() {
                        step_simulation(&mut editor, 1);
                    }
                    if ui
                        .button(if editor.running { "Pause" } else { "Run" })
                        .clicked()
                    {
                        toggle_simulation(&mut editor);
                    }
                    if ui.button("Reset").clicked() {
                        reset_simulation(&mut editor);
                    }
                });
                ui.add(
                    egui::Slider::new(&mut editor.ticks_per_second, 1.0..=100.0).text("ticks/sec"),
                );
                if let Some(simulation) = editor.simulation.as_ref() {
                    ui.label(format!(
                        "tick {} / {} changes / {}",
                        simulation.tick(),
                        simulation.change_count(),
                        if simulation.is_quiescent() {
                            "quiet"
                        } else {
                            "scheduled"
                        }
                    ));
                    if let Ok(changes) = simulation.changes() {
                        ui.collapsing("Block changes", |ui| {
                            for change in changes.iter().rev().take(12).rev() {
                                ui.small(format!(
                                    "t{} ({},{},{}) {} → {}",
                                    change.tick,
                                    change.pos[0],
                                    change.pos[1],
                                    change.pos[2],
                                    short_state(&change.from),
                                    short_state(&change.to)
                                ));
                            }
                        });
                    }
                }
                if !editor.probe_history.is_empty() {
                    ui.collapsing("Selected probe", |ui| {
                        for (tick, state) in editor.probe_history.iter().rev().take(8).rev() {
                            ui.small(format!("t{tick}: {}", short_state(state)));
                        }
                    });
                }
                if !editor.brewing_log.is_empty() {
                    ui.collapsing("Brewing events", |ui| {
                        for event in editor.brewing_log.iter().rev().take(8).rev() {
                            ui.small(event);
                        }
                    });
                }
                let diagnostics = crate::simulation::diagnostics(&editor.document);
                if !diagnostics.is_empty() {
                    ui.collapsing(format!("Diagnostics ({})", diagnostics.len()), |ui| {
                        for diagnostic in diagnostics.iter().take(8) {
                            ui.small(format!(
                                "{:?} ({},{},{}): {}",
                                diagnostic.level,
                                diagnostic.pos.x,
                                diagnostic.pos.y,
                                diagnostic.pos.z,
                                diagnostic.message
                            ));
                        }
                    });
                }
            });
        });
    egui::Panel::bottom("status").show(&mut root, |ui| {
        ui.label(&editor.message);
    });
    viewport_bounds.0 = Some(root.available_rect_before_wrap());

    if editor.confirm_discard {
        egui::Window::new("未保存の変更")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("未保存の変更があります。");
                ui.horizontal(|ui| {
                    if ui.button("破棄して続行").clicked() {
                        let action = editor.pending_file_action.take();
                        editor.document.dirty = false;
                        editor.confirm_discard = false;
                        if let Some(action) = action {
                            execute_file_action(&mut editor, action);
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        editor.confirm_discard = false;
                        editor.pending_file_action = None;
                    }
                });
            });
    }
    if editor.confirm_close {
        egui::Window::new("終了確認")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("未保存の変更があります。");
                ui.horizontal(|ui| {
                    if ui.button("Saveして終了").clicked() {
                        save_document(&mut editor);
                        if !editor.document.dirty {
                            exit.write(AppExit::Success);
                        }
                    }
                    if ui.button("破棄して終了").clicked() {
                        exit.write(AppExit::Success);
                    }
                    if ui.button("Cancel").clicked() {
                        editor.confirm_close = false;
                    }
                });
            });
    }
    Ok(())
}

fn sync_inventory_editor(editor: &mut Editor) {
    if editor.inventory_for == editor.selected {
        return;
    }
    editor.inventory_for = editor.selected;
    editor.inventory_draft.clear();
    let Some(pos) = editor.selected else {
        return;
    };
    let Some(slots) = editor.document.inventory_slot_count(pos) else {
        return;
    };
    let existing = editor.document.inventory(pos);
    for slot in 0..slots {
        editor.inventory_draft.push(
            existing
                .iter()
                .find(|item| usize::from(item.slot) == slot)
                .cloned()
                .unwrap_or_else(|| InventoryItem::new(slot as u8, "", 0)),
        );
    }
}

fn brewing_preset(draft: &mut Vec<InventoryItem>, potion: &str, ingredient: &str) {
    draft.clear();
    for slot in 0..3 {
        draft.push(InventoryItem::new(slot, "minecraft:potion", 1).with_potion(potion));
    }
    draft.push(InventoryItem::new(3, ingredient, 1));
    draft.push(InventoryItem::new(4, "minecraft:blaze_powder", 1));
}

fn brew_event_label(event: BrewEvent) -> &'static str {
    match event {
        BrewEvent::FuelLoaded => "燃料を補充",
        BrewEvent::Started => "醸造開始",
        BrewEvent::Cancelled => "材料変更により中断",
        BrewEvent::Completed => "醸造完了",
    }
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "udev-gothic".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/UDEVGothic-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "jetbrains-mono".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMono-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "udev-gothic".to_owned());
    let monospace = fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default();
    monospace.insert(0, "udev-gothic".to_owned());
    monospace.insert(0, "jetbrains-mono".to_owned());
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.indent = 16.0;
    style.spacing.slider_width = 128.0;
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(19.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );
    style.visuals.panel_fill = egui::Color32::from_rgb(24, 27, 34);
    style.visuals.window_fill = egui::Color32::from_rgb(28, 32, 40);
    ctx.set_style_of(egui::Theme::Dark, style);
}

fn request_file_action(editor: &mut Editor, action: FileAction) {
    if editor.document.dirty {
        editor.pending_file_action = Some(action);
        editor.confirm_discard = true;
    } else {
        execute_file_action(editor, action);
    }
}

fn execute_file_action(editor: &mut Editor, action: FileAction) {
    match action {
        FileAction::New => {
            editor.replace_document(Document::default());
            editor.active_layer = 1;
            editor.message = "新規プロジェクト".to_owned();
        }
        FileAction::Open => open_document(editor),
    }
}

fn open_document(editor: &mut Editor) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Litematic", &["litematic"])
        .pick_file()
    else {
        editor.message = "Openをキャンセルしました".to_owned();
        return;
    };
    match Document::open(&path) {
        Ok(document) => {
            editor.replace_document(document);
            editor.message = format!("{} を開きました", path.display());
        }
        Err(error) => editor.message = error,
    }
}

fn save_document(editor: &mut Editor) {
    let Some(path) = editor.document.path().map(|path| path.to_path_buf()) else {
        save_document_as(editor);
        return;
    };
    save_document_to(editor, path);
}

fn save_document_as(editor: &mut Editor) {
    let suggested_name = editor
        .document
        .path()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("design.litematic");
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Litematic", &["litematic"])
        .set_file_name(suggested_name)
        .save_file()
    else {
        editor.message = "Saveをキャンセルしました".to_owned();
        return;
    };
    let path = if path.extension().is_some() {
        path
    } else {
        path.with_extension("litematic")
    };
    save_document_to(editor, path);
}

fn save_document_to(editor: &mut Editor, path: PathBuf) {
    match editor.document.save(&path) {
        Ok(()) => editor.message = format!("{} を保存しました", path.display()),
        Err(error) => editor.message = error,
    }
}

fn block_vec3(pos: BlockPos) -> Vec3 {
    Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32)
}

fn state_property<'a>(state: &'a str, key: &str) -> Option<&'a str> {
    let properties = state.split_once('[')?.1.strip_suffix(']')?;
    properties.split(',').find_map(|pair| {
        let (candidate, value) = pair.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn facing_marker(state: &str) -> Option<(Quat, Vec3)> {
    let facing = state_property(state, "facing")?;
    Some(match facing {
        "north" => (Quat::IDENTITY, Vec3::NEG_Z),
        "east" => (Quat::from_rotation_y(FRAC_PI_2), Vec3::X),
        "south" => (Quat::from_rotation_y(std::f32::consts::PI), Vec3::Z),
        "west" => (Quat::from_rotation_y(-FRAC_PI_2), Vec3::NEG_X),
        "up" => (Quat::from_rotation_x(FRAC_PI_2), Vec3::Y),
        "down" => (Quat::from_rotation_x(FRAC_PI_2), Vec3::NEG_Y),
        _ => return None,
    })
}

fn short_state(state: &str) -> String {
    let state = state.strip_prefix("minecraft:").unwrap_or(state);
    if state.chars().count() > 52 {
        format!("{}…", state.chars().take(51).collect::<String>())
    } else {
        state.to_owned()
    }
}

fn short_palette_label(label: &str) -> String {
    let initials: String = label
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect();
    if initials.chars().count() > 1 {
        initials
    } else {
        label.chars().take(2).collect()
    }
}

fn block_shape(state: &str) -> (Vec3, Vec3) {
    if state.contains("redstone_wire") || state.contains("pressure_plate") {
        (Vec3::new(0.82, 0.06, 0.82), Vec3::new(0.0, -0.43, 0.0))
    } else if state.contains("button") {
        (Vec3::new(0.42, 0.14, 0.42), Vec3::new(0.0, -0.36, 0.0))
    } else if state.contains("torch") {
        (Vec3::new(0.16, 0.7, 0.16), Vec3::new(0.0, -0.12, 0.0))
    } else if state.contains("repeater") || state.contains("comparator") {
        (Vec3::new(0.86, 0.18, 0.86), Vec3::new(0.0, -0.36, 0.0))
    } else if state.contains("brewing_stand") {
        (Vec3::new(0.58, 0.84, 0.58), Vec3::new(0.0, -0.06, 0.0))
    } else if state.contains("hopper") {
        (Vec3::new(0.78, 0.68, 0.78), Vec3::new(0.0, -0.12, 0.0))
    } else {
        (Vec3::ONE, Vec3::ZERO)
    }
}

fn block_color(state: &str) -> Color {
    if state.contains("redstone_wire") {
        let power = state_property(state, "power")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        Color::srgb(0.3 + power / 15.0 * 0.65, 0.025, 0.02)
    } else if state.contains("redstone_block") {
        Color::srgb(0.82, 0.04, 0.035)
    } else if state.contains("lamp") && state.contains("lit=true") {
        Color::srgb(1.0, 0.76, 0.12)
    } else if state.contains("powered=true") {
        Color::srgb(1.0, 0.28, 0.08)
    } else if state.contains("torch") {
        Color::srgb(0.95, 0.18, 0.08)
    } else if state.contains("repeater") {
        Color::srgb(0.82, 0.76, 0.65)
    } else if state.contains("comparator") {
        Color::srgb(0.58, 0.47, 0.42)
    } else if state.contains("observer") {
        Color::srgb(0.38, 0.42, 0.46)
    } else if state.contains("piston") {
        Color::srgb(0.58, 0.43, 0.2)
    } else if state.contains("lamp") {
        Color::srgb(0.58, 0.32, 0.08)
    } else if state.contains("hopper") {
        Color::srgb(0.25, 0.29, 0.34)
    } else if state.contains("water") {
        Color::srgba(0.08, 0.34, 0.9, 0.75)
    } else if state.contains("brewing_stand") {
        Color::srgb(0.45, 0.3, 0.42)
    } else if state.contains("button") {
        Color::srgb(0.55, 0.58, 0.62)
    } else {
        Color::srgb(0.32, 0.36, 0.42)
    }
}
