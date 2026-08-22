use std::{
    collections::BTreeSet,
    f32::consts::FRAC_PI_2,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::{
    app::AppExit,
    camera::ScalingMode,
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::{AccumulatedMouseMotion, MouseWheel},
    prelude::*,
    window::{WindowCloseRequested, WindowResolution},
};
use bevy_egui::{
    EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui,
    input::{egui_wants_any_keyboard_input, egui_wants_any_pointer_input},
};

use crate::{
    APP_NAME,
    document::{BlockPos, Document, InventoryItem, rotate_state_y},
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
    file_path: String,
    message: String,
    scene_epoch: u64,
    confirm_discard: bool,
    confirm_close: bool,
}

impl Editor {
    fn replace_document(&mut self, document: Document) {
        self.active_layer = document.minimum().y;
        self.file_path = document
            .path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "design.litematic".to_owned());
        self.document = document;
        self.simulation = None;
        self.running = false;
        self.probe_history.clear();
        self.selected = None;
        self.selection = None;
        self.selection_anchor = None;
        self.inventory_for = None;
        self.inventory_draft.clear();
        self.scene_epoch = self.scene_epoch.wrapping_add(1);
        self.confirm_discard = false;
    }
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
            file_path: "design.litematic".to_owned(),
            message: "Gate 0 fixtureを読み込みました".to_owned(),
            scene_epoch: 0,
            confirm_discard: false,
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
        .init_resource::<PaintDrag>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("{APP_NAME} — Technical Preview"),
                resolution: WindowResolution::new(1280, 800),
                ..default()
            }),
            close_when_requested: false,
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, sync_scene)
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

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.7, 0.0)),
    ));
    commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 12.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(8.0, 9.0, 10.0).looking_at(Vec3::new(1.0, 0.0, 0.0), Vec3::Y),
        MainCamera,
    ));
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
    mut rendered: ResMut<RenderedRevision>,
    visuals: Query<Entity, With<SceneVisual>>,
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
    for entity in &visuals {
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
        let state = editor
            .simulation
            .as_ref()
            .map(|simulation| simulation.block(pos))
            .unwrap_or_else(|| editor.document.block(pos));
        if state == AIR {
            continue;
        }
        let (size, offset) = block_shape(&state);
        let selected = editor.selected == Some(pos);
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: if selected {
                    Color::srgb(0.12, 0.82, 0.92)
                } else {
                    block_color(&state)
                },
                alpha_mode: if state.starts_with("minecraft:water") {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                perceptual_roughness: 0.72,
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
                MeshMaterial3d(materials.add(Color::srgb(0.98, 0.82, 0.16))),
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
                MeshMaterial3d(materials.add(Color::srgb(1.0, 0.25, 0.05))),
                Transform::from_translation(block_vec3(pos) + Vec3::new(0.34, height * 0.5, 0.34)),
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
    mut cursor: ResMut<CursorCell>,
    cursor_preview: Single<(&mut Transform, &mut Visibility), With<CursorPreview>>,
    selection_preview: SelectionPreviewQuery,
) {
    cursor.0 = if editor.camera == CameraPreset::Orbit {
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
        match rotate_state_y(&editor.paint_state) {
            Ok(state) => editor.paint_state = state,
            Err(error) => editor.message = error,
        }
    }
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        let result = if shift {
            editor.document.redo()
        } else {
            editor.document.undo()
        };
        if let Err(error) = result {
            editor.message = error;
        }
        editor.simulation = None;
    }
    if ctrl
        && keys.just_pressed(KeyCode::KeyC)
        && let Some((a, b)) = editor.selection
    {
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
    if ctrl
        && keys.just_pressed(KeyCode::KeyV)
        && let Some(origin) = cursor.0
    {
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
    if keys.just_pressed(KeyCode::Delete)
        && let Some((a, b)) = editor.selection
    {
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
    if keys.just_pressed(KeyCode::Escape) {
        editor.selection = None;
        editor.selection_anchor = None;
    }
    if keys.just_pressed(KeyCode::KeyF) {
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
    if keys.just_pressed(KeyCode::Space) {
        if editor.simulation.is_none() {
            start_simulation(&mut editor);
        } else {
            editor.running = !editor.running;
            editor.message = if editor.running {
                "simulationを実行中".to_owned()
            } else {
                "simulationをpauseしました".to_owned()
            };
        }
    }
    if keys.just_pressed(KeyCode::Period) {
        step_simulation(&mut editor, 1);
    }
}

fn start_simulation(editor: &mut Editor) {
    match RedstoneSimulation::start(&editor.document, editor.settle_mode, 0) {
        Ok(simulation) => {
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
    let Some(simulation) = editor.simulation.as_mut() else {
        return;
    };
    simulation.run(ticks);
    editor.message = format!("tick {}", simulation.tick());
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
    mut exit: MessageWriter<AppExit>,
    mut fonts_configured: Local<bool>,
) -> Result {
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
    egui::Panel::top("toolbar").show(&mut root, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.heading(APP_NAME);
            if editor.document.dirty {
                ui.label("● unsaved");
            }
            ui.separator();
            if ui.button("New").clicked() {
                if editor.document.dirty {
                    editor.confirm_discard = true;
                } else {
                    editor.replace_document(Document::default());
                }
            }
            ui.add(egui::TextEdit::singleline(&mut editor.file_path).desired_width(230.0));
            if ui.button("Open").clicked() {
                if editor.document.dirty {
                    editor.confirm_discard = true;
                } else {
                    open_document(&mut editor);
                }
            }
            if ui.button("Save").clicked() {
                save_document(&mut editor);
            }
            ui.separator();
            if ui
                .add_enabled(editor.document.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                if let Err(error) = editor.document.undo() {
                    editor.message = error;
                }
                editor.simulation = None;
            }
            if ui
                .add_enabled(editor.document.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                if let Err(error) = editor.document.redo() {
                    editor.message = error;
                }
                editor.simulation = None;
            }
            ui.separator();
            ui.selectable_value(&mut editor.camera, CameraPreset::Top, "Top");
            ui.selectable_value(&mut editor.camera, CameraPreset::Isometric, "Isometric");
            ui.selectable_value(&mut editor.camera, CameraPreset::Orbit, "Orbit");
            ui.separator();
            if ui.button("[").clicked() {
                editor.active_layer -= 1;
            }
            ui.label(format!("Y {}", editor.active_layer));
            if ui.button("]").clicked() {
                editor.active_layer += 1;
            }
        });
    });
    egui::Panel::left("palette")
        .resizable(false)
        .show(&mut root, |ui| {
            ui.heading("Palette");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut editor.tool, Tool::Paint, "Paint");
                ui.selectable_value(&mut editor.tool, Tool::Select, "Select (V)");
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (index, item) in PALETTE.iter().enumerate() {
                    if ui
                        .selectable_label(editor.palette_index == index, item.label)
                        .clicked()
                    {
                        editor.palette_index = index;
                        editor.paint_state = item.state.to_owned();
                        editor.tool = Tool::Paint;
                    }
                }
            });
            ui.separator();
            ui.label("R: facingを90°回転");
            ui.small("左drag: 配置 / 右drag: 削除");
        });
    egui::Panel::right("inspector")
        .resizable(true)
        .show(&mut root, |ui| {
            ui.heading("Inspector");
            if let Some(pos) = editor.selected {
                ui.monospace(format!("x={} y={} z={}", pos.x, pos.y, pos.z));
                let state = editor
                    .simulation
                    .as_ref()
                    .map(|simulation| simulation.block(pos))
                    .unwrap_or_else(|| editor.document.block(pos));
                ui.monospace(state);
                if editor.document.inventory_slot_count(pos).is_some() {
                    ui.collapsing("Inventory", |ui| {
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
                                        ui.add(egui::DragValue::new(&mut item.count).range(0..=64));
                                    });
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
                                    editor.running = false;
                                    editor.inventory_for = None;
                                }
                                Ok(false) => editor.message = "Inventoryは変更なしです".to_owned(),
                                Err(error) => editor.message = error,
                            }
                        }
                    });
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
                    ui.selectable_value(&mut editor.settle_mode, SettleMode::InWorld, "InWorld");
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
                if ui.button("Use").clicked()
                    && let (Some(pos), Some(simulation)) =
                        (editor.selected, editor.simulation.as_mut())
                {
                    simulation.use_block(pos);
                    editor.scene_epoch = editor.scene_epoch.wrapping_add(1);
                    record_probe(&mut editor);
                }
                if ui.button("Step").clicked() {
                    step_simulation(&mut editor, 1);
                }
                if ui
                    .button(if editor.running { "Pause" } else { "Run" })
                    .clicked()
                    && editor.simulation.is_some()
                {
                    editor.running = !editor.running;
                }
                if ui.button("Reset").clicked() {
                    editor.simulation = None;
                    editor.running = false;
                    editor.probe_history.clear();
                    editor.scene_epoch = editor.scene_epoch.wrapping_add(1);
                }
            });
            ui.add(egui::Slider::new(&mut editor.ticks_per_second, 1.0..=100.0).text("ticks/sec"));
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
    egui::Panel::bottom("status").show(&mut root, |ui| {
        ui.label(&editor.message);
    });

    if editor.confirm_discard {
        egui::Window::new("未保存の変更")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("未保存の変更があります。破棄する操作を選んでください。");
                ui.horizontal(|ui| {
                    if ui.button("破棄してNew").clicked() {
                        editor.replace_document(Document::default());
                    }
                    if ui.button("破棄してOpen").clicked() {
                        editor.document.dirty = false;
                        open_document(&mut editor);
                    }
                    if ui.button("Cancel").clicked() {
                        editor.confirm_discard = false;
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
                .unwrap_or(InventoryItem {
                    slot: slot as u8,
                    id: String::new(),
                    count: 0,
                }),
        );
    }
}

fn configure_fonts(ctx: &egui::Context) {
    let candidates = [
        Path::new(r"C:\Windows\Fonts\NotoSansJP-VF.ttf"),
        Path::new(r"C:\Windows\Fonts\meiryo.ttc"),
    ];
    let Some(bytes) = candidates.iter().find_map(|path| std::fs::read(path).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "redforge-japanese".to_owned(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "redforge-japanese".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn open_document(editor: &mut Editor) {
    let path = normalized_path(&editor.file_path);
    match Document::open(&path) {
        Ok(document) => {
            editor.replace_document(document);
            editor.message = format!("{} を開きました", path.display());
        }
        Err(error) => editor.message = error,
    }
}

fn save_document(editor: &mut Editor) {
    let path = normalized_path(&editor.file_path);
    match editor.document.save(&path) {
        Ok(()) => {
            editor.file_path = path.display().to_string();
            editor.message = format!("{} を保存しました", path.display());
        }
        Err(error) => editor.message = error,
    }
}

fn normalized_path(text: &str) -> PathBuf {
    let path = Path::new(text.trim());
    if path.extension().is_none() {
        path.with_extension("litematic")
    } else {
        path.to_owned()
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
        (Vec3::splat(0.92), Vec3::ZERO)
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
