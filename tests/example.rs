use bevy::DefaultPlugins;
use bevy::app::App;
use bevy::asset::asset_value;
use bevy::camera::Camera3d;
use bevy::color::Color;
use bevy::light::PointLight;
use bevy::math::{Quat, Vec3};
use bevy::mesh::Mesh3d;
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::{Circle, Commands, CommandsSceneExt, Cuboid, ResMut, SceneList, Transform, bsn_list, template_value, With, Query};
use bevy_mod_screenshot_test::{ScreenshotTestConfig, ScreenshotTestPlugin, TestTask, TestTasks};
use log::info;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(ScreenshotTestPlugin);

    TestTasks::for_app(&mut app)
        .with_config(ScreenshotTestConfig::default())
        .run("initialize", |mut task: ResMut<TestTask>| {
            info!("doing... nothing!");
            task.finish();
        })
        .take_screenshot("example_empty")
        .run(
            "add scene",
            |mut commands: Commands, mut task: ResMut<TestTask>| {
                commands.spawn_scene_list(scene());
                task.finish();
            },
        )
        // only works for pipelines
        .await_assets()
        // TODO: figure out a better way to wait for scene to be loaded.
        .sleep(10)
        .take_screenshot("example_scene")
        .run(
            "rotate",
            rotate
        )
        .take_screenshot("example_rotated")
        .validate();
}
fn rotate(mut task: ResMut<TestTask>, query: Query<&mut Transform, With<Camera3d>>){
    for mut transform in query {
        *transform = Transform::from_xyz(2.5, 4.5, -9.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    task.finish()
}

fn scene() -> impl SceneList {
    bsn_list! [
        (
            #CircularBase
            Mesh3d(asset_value(Circle::new(4.0)))
            MeshMaterial3d::<StandardMaterial>(asset_value(Color::WHITE))
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
        ),
        (
            #Cube
            Mesh3d(asset_value(Cuboid::new(1.0, 1.0, 1.0)))
            MeshMaterial3d::<StandardMaterial>(asset_value(Color::srgb_u8(124, 144, 255)))
            Transform::from_xyz(0.0, 0.5, 0.0)
        ),
        (
            PointLight {
                shadow_maps_enabled: true,
            }
            Transform::from_xyz(4.0, 8.0, 4.0)
        ),
        (
            Camera3d
            template_value(Transform::from_xyz(2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y))
        )
    ]
}
