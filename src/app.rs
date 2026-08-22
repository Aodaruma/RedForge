use bevy::{core_pipeline::tonemapping::Tonemapping, prelude::*, window::WindowResolution};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use crate::{
    APP_NAME,
    document::{BlockPos, Document},
    simulation::{RedstoneSimulation, SettleMode},
};

#[derive(Resource)]
struct Editor {
    document: Document,
    simulation: Option<RedstoneSimulation>,
    selected: Option<BlockPos>,
    camera: CameraPreset,
    message: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CameraPreset {
    Top,
    #[default]
    Isometric,
    Orbit,
}

#[derive(Component)]
struct BlockVisual;

#[derive(Component)]
struct MainCamera;

#[derive(Resource, Default)]
struct RenderedRevision(u64);

pub fn run() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.035, 0.045, 0.06)))
        .insert_resource(Editor {
            document: Document::gate_zero_fixture().expect("built-in fixture must be valid"),
            simulation: None,
            selected: Some(BlockPos::new(0, 1, 0)),
            camera: CameraPreset::Isometric,
            message: "Gate 0 fixtureを読み込みました".to_owned(),
        })
        .init_resource::<RenderedRevision>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("{APP_NAME} — Technical Preview"),
                resolution: WindowResolution::new(1280, 800),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (sync_scene, camera_keys))
        .add_systems(EguiPrimaryContextPass, editor_ui)
        .run();
}

fn setup(mut commands: Commands) {
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
            scale: 9.0,
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(8.0, 9.0, 10.0).looking_at(Vec3::new(1.0, 0.0, 0.0), Vec3::Y),
        MainCamera,
    ));
}

fn sync_scene(
    mut commands: Commands,
    editor: Res<Editor>,
    mut rendered: ResMut<RenderedRevision>,
    visuals: Query<Entity, With<BlockVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if rendered.0 == editor.document.revision {
        return;
    }
    for entity in &visuals {
        commands.entity(entity).despawn();
    }
    let mesh = meshes.add(Cuboid::new(0.92, 0.92, 0.92));
    for (pos, state) in editor.document.blocks() {
        let color = block_color(&state);
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.72,
                ..default()
            })),
            Transform::from_xyz(pos.x as f32, pos.y as f32, pos.z as f32),
            BlockVisual,
        ));
    }
    rendered.0 = editor.document.revision;
}

fn camera_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: ResMut<Editor>,
    mut camera: Single<&mut Transform, With<MainCamera>>,
    mut applied: Local<CameraPreset>,
) {
    if keys.just_pressed(KeyCode::Digit1) {
        editor.camera = CameraPreset::Top;
    } else if keys.just_pressed(KeyCode::Digit2) {
        editor.camera = CameraPreset::Isometric;
    } else if keys.just_pressed(KeyCode::Digit3) {
        editor.camera = CameraPreset::Orbit;
    }
    if *applied != editor.camera {
        set_camera(&mut camera, editor.camera);
        *applied = editor.camera;
    }
}

fn editor_ui(mut contexts: EguiContexts, mut editor: ResMut<Editor>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::Panel::top("toolbar").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.heading(APP_NAME);
            ui.separator();
            ui.label("View:");
            for (label, preset) in [
                ("1 Top", CameraPreset::Top),
                ("2 Isometric", CameraPreset::Isometric),
                ("3 Orbit", CameraPreset::Orbit),
            ] {
                if ui
                    .selectable_label(editor.camera == preset, label)
                    .clicked()
                {
                    editor.camera = preset;
                }
            }
            ui.separator();
            if ui.button("Start").clicked() {
                match RedstoneSimulation::start(&editor.document, SettleMode::InWorld, 0) {
                    Ok(simulation) => {
                        editor.message = simulation.summary();
                        editor.simulation = Some(simulation);
                    }
                    Err(error) => editor.message = error,
                }
            }
            if ui.button("Use selected").clicked()
                && let (Some(pos), Some(simulation)) = (editor.selected, editor.simulation.as_mut())
            {
                simulation.use_block(pos);
                editor.message = format!("{pos:?} を操作しました");
            }
            if ui.button("Step").clicked()
                && let Some(simulation) = editor.simulation.as_mut()
            {
                simulation.step();
                editor.message = format!("tick {}", simulation.tick());
            }
            if ui.button("Reset").clicked() {
                editor.simulation = None;
                editor.message = "simulationをresetしました".to_owned();
            }
        });
    });
    egui::Panel::left("palette").show(&mut root, |ui| {
        ui.heading("Gate 0");
        ui.label("button → dust → sticky piston");
        ui.separator();
        ui.label("1 / 2 / 3: camera切替");
        ui.label("Start → Use selected → Step");
    });
    egui::Panel::right("inspector").show(&mut root, |ui| {
        ui.heading("Inspector");
        if let Some(pos) = editor.selected {
            ui.monospace(format!("{}, {}, {}", pos.x, pos.y, pos.z));
            let state = editor
                .simulation
                .as_ref()
                .map(|sim| sim.block(pos))
                .unwrap_or_else(|| editor.document.block(pos));
            ui.monospace(state);
        }
    });
    egui::Panel::bottom("status").show(&mut root, |ui| {
        ui.label(&editor.message);
    });
    Ok(())
}

fn set_camera(transform: &mut Transform, preset: CameraPreset) {
    *transform = match preset {
        CameraPreset::Top => {
            Transform::from_xyz(1.0, 14.0, 0.001).looking_at(Vec3::new(1.0, 0.0, 0.0), Vec3::NEG_Z)
        }
        CameraPreset::Isometric => {
            Transform::from_xyz(8.0, 9.0, 10.0).looking_at(Vec3::new(1.0, 0.0, 0.0), Vec3::Y)
        }
        CameraPreset::Orbit => {
            Transform::from_xyz(10.0, 5.0, -11.0).looking_at(Vec3::new(1.0, 0.0, 0.0), Vec3::Y)
        }
    };
}

fn block_color(state: &str) -> Color {
    if state.contains("redstone_wire") {
        Color::srgb(0.65, 0.05, 0.04)
    } else if state.contains("piston") {
        Color::srgb(0.55, 0.42, 0.22)
    } else if state.contains("button") {
        Color::srgb(0.55, 0.58, 0.62)
    } else {
        Color::srgb(0.32, 0.36, 0.42)
    }
}
