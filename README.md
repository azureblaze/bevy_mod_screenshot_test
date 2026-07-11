# bevy_mod_screenshot_test

Bevy plugin for running screenshot integration tests.

Screenshot tests allows visual regressions to be detected in cases regular unit tests does not work well (or at all),
especially rendering. Otherwise, they are slow and flaky and should be avoided when possible.

`bevy_mod_screenshot_test` does the following things:

1. User defines a series of tasks (user defined Bevy systems that runs once sequentially) to perform in the bevy app,
   such as spawning entities and when to take the screenshot.
2. The plugin starts the app, runs through the defined tasks, and end the app automatically.
3. On the first test run the plugin writes all captured screenshots into the `expected` directory, which the user will
   verify manually and commit the image.
4. On subsequent runs the plugin will compare the captured screenshots with the ones in the `expected` directory, and
   the test will fail if any of them does not match.
5. The user can then fix their implementation, or update the screenshot in the `expected` directory if the change is
   deemed intentional.

# Example

## `Cargo.toml`

Bevy apps cannot run under the standard Rust test harness (which trys to start bevy on non-main thread and upsets
`winit`), hence the harness must be manually turned off.

```ignore
[[test]]
name="example"
path="tests/example.rs"
harness = false
```

## `tests/example.rs`

```ignore
// provide your own entry point as #[test] won't work.
fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(ScreenshotTestPlugin);

    TestTasks::for_app(&mut app)
        //optional
        .with_config(ScreenshotTestConfig::default())
        .run(
            // label for the task which will be logged when it finishes.
            "add scene",
            // a lambda system
            |mut commands: Commands, mut task: ResMut<TestTask>| {
                commands.spawn_scene_list(scene());
                // must call task.finish() to signal the task has completed.  This might be done asynchonously in a 
                // observer in complicated cases. 
                task.finish();
            },
        )
        // wait for shaders to finish compiling. unfortunately other assets are not handled at this moment.
        .await_assets()
        // wait for another 10 frames for the scene to actually spawn in.
        .sleep(10)
        // saves the screenshot to './tests/example_scene.png'
        .take_screenshot("example_scene")
        // change more things and take more screenshots. initializing the app/assets is expensive and it is better to 
        // run more test cases together against the same asset. 
        .run(
            "rotate",
            rotate
        )
        .take_screenshot("example_rotated")
        // finish the test. end the bevy app and compare the screenshots.
        .validate();
}

// a regular system
fn rotate(mut task: ResMut<TestTask>, query: Query<&mut Transform, With<Camera3d>>){
    for mut transform in query {
        *transform = Transform::from_xyz(2.5, 4.5, -9.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    // don't forget to call this or the test will hang.
    task.finish()
}

fn scene() -> impl SceneList {
    bsn_list! [
        // ...
    ]
}
```
