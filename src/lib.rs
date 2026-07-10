use crate::pipelines_ready::{PipelinesReady, PipelinesReadyPlugin};
use anstyle::AnsiColor;
use bevy::app::{App, AppExit, Plugin, Update};
use bevy::diagnostic::FrameCount;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::{
    Commands, IntoScheduleConfigs, On, Res, ResMut, Resource, SystemCondition, default, run_once,
};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use log::info;
use rendiff::Threshold;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Resource, Clone)]
pub struct ScreenshotTestConfig {
    pub expected_dir: PathBuf,
    pub actual_dir: PathBuf,
    pub diff_dir: PathBuf,
}

impl ScreenshotTestConfig {
    fn expected(&self, name: &str) -> PathBuf {
        self.expected_dir.join(format!("{name}.png"))
    }
    fn actual(&self, name: &str) -> PathBuf {
        self.actual_dir.join(format!("{name}.png"))
    }
    fn diff(&self, name: &str) -> PathBuf {
        self.diff_dir.join(format!("{name}.png"))
    }
}

impl Default for ScreenshotTestConfig {
    fn default() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        ScreenshotTestConfig {
            expected_dir: root.join("expected"),
            actual_dir: root.join("actual"),
            diff_dir: root.join("diff"),
        }
    }
}

#[derive(Default, Clone)]
pub struct TaskInfo {
    pub name: String,
    pub screenshot: String,
}

#[derive(Default)]
pub struct ScreenshotTestPlugin;

impl Plugin for ScreenshotTestPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PipelinesReadyPlugin)
            .add_systems(Update, run_task.run_if(should_run_next_task));
    }
}
pub struct TestTasks<'a> {
    app: &'a mut App,
    tasks: VecDeque<(usize, TaskInfo)>,
    config: ScreenshotTestConfig,
}

impl TestTasks<'_> {
    pub fn for_app(app: &mut App) -> TestTasks<'_> {
        TestTasks {
            app,
            tasks: VecDeque::default(),
            config: ScreenshotTestConfig::default(),
        }
    }

    pub fn with_config(&mut self, config: ScreenshotTestConfig) -> &mut Self {
        self.config = config;
        self
    }

    fn run_impl<M>(
        &mut self,
        task_info: TaskInfo,
        task: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        let task_id = self.tasks.len();

        self.app.add_systems(
            Update,
            task.after(run_task)
                .run_if(current_task(task_id).and_then(run_once)),
        );

        self.tasks.push_back((task_id, task_info));
        self
    }

    pub fn run_unnamed<M>(
        &mut self,
        task: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.run_impl(TaskInfo::default(), task)
    }

    pub fn run<M>(
        &mut self,
        name: &str,
        task: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.run_impl(
            TaskInfo {
                name: name.to_owned(),
                ..default()
            },
            task,
        )
    }

    pub fn take_screenshot(&mut self, name: &str) -> &mut Self {
        self.run_impl(
            TaskInfo {
                name: format!("take screenshot '{}'", name),
                screenshot: name.to_owned(),
            },
            noop(),
        )
    }

    pub fn await_assets(&mut self) -> &mut Self {
        let task_info = TaskInfo {
            name: "await assets loaded".to_owned(),
            ..default()
        };

        let task_id = self.tasks.len();

        self.app.add_systems(
            Update,
            noop().after(run_task).run_if(
                current_task(task_id)
                    .and_then(assets_loaded)
                    .and_then(run_once),
            ),
        );

        self.tasks.push_back((task_id, task_info));
        self
    }
    pub fn noop(&mut self) -> &mut Self {
        self.run_impl(
            TaskInfo {
                name: "noop".to_owned(),
                ..default()
            },
            noop(),
        )
    }

    pub fn sleep(&mut self, frames: u32) -> &mut Self {
        self.run(
            &format!("set sleep target +{frames} frames"),
            move |mut task: ResMut<TestTask>, frame_count: Res<FrameCount>| {
                task.sleep_target = frame_count.0 + frames;
                task.finish();
            },
        );

        let task_info = TaskInfo {
            name: format!("sleep {frames} frames"),
            ..default()
        };

        let task_id = self.tasks.len();

        self.app.add_systems(
            Update,
            noop().after(run_task).run_if(
                current_task(task_id)
                    .and_then(sleep_done)
                    .and_then(run_once),
            ),
        );

        self.tasks.push_back((task_id, task_info));
        self
    }

    pub fn validate(&mut self) {
        for (_, task) in &self.tasks {
            if !task.screenshot.is_empty() {
                let file = self.config.actual(&task.screenshot);
                if file.exists() {
                    info!("cleaning {}", file.to_str().unwrap());
                    fs::remove_file(file).unwrap();
                }
            }
        }

        let state = TestTask {
            tasks: self.tasks.clone(),
            ..default()
        };

        self.app
            .insert_resource(state)
            .insert_resource(self.config.clone());

        self.app.run();
        self.validate_impl();
    }

    fn validate_impl(&self) {
        let mut failed = false;

        let red = anstyle::Style::new().fg_color(Some(AnsiColor::Red.into()));
        let yellow = anstyle::Style::new().fg_color(Some(AnsiColor::Yellow.into()));
        let green = anstyle::Style::new().fg_color(Some(AnsiColor::Green.into()));

        for (_, task) in &self.tasks {
            if task.screenshot.is_empty() {
                continue;
            }

            let actual = self.config.actual(&task.screenshot);
            if !actual.exists() {
                println!(
                    "[{red}FAIL{red:#}] screenshot '{}' not taken.",
                    task.screenshot
                );
                failed = true;
                continue;
            }

            let expected = self.config.expected(&task.screenshot);
            if !expected.exists() {
                fs::create_dir_all(expected.parent().unwrap()).unwrap();
                fs::copy(actual, expected).unwrap();
                println!(
                    "[{yellow}NEW {yellow:#}] created expected screenshot '{}' from actual.",
                    task.screenshot
                );
                continue;
            }

            let actual = from_rgba(image::open(actual).unwrap().to_rgba8());
            let expected = from_rgba(image::open(expected).unwrap().to_rgba8());

            let difference = rendiff::diff(actual.as_ref(), expected.as_ref());
            if !Threshold::no_bigger_than(1).allows(difference.histogram()) {
                let Some(diff_image) = difference.diff_image() else {
                    panic!()
                };
                let diff = self.config.diff(&task.screenshot);
                fs::create_dir_all(diff.parent().unwrap()).unwrap_or(());
                image::RgbaImage::from_raw(
                    diff_image.width() as u32,
                    diff_image.height() as u32,
                    Vec::from(diff_image.buf().as_flattened()),
                )
                .unwrap()
                .save(diff.clone())
                .unwrap();

                failed = true;
                println!(
                    "[{red}FAIL{red:#}] screenshot '{}' mismatch: {}",
                    task.screenshot,
                    diff.to_str().unwrap()
                );
                continue;
            }
            println!(
                "[{green}PASS{green:#}] screenshot '{}'",
                task.screenshot
            );
        }

        if failed {
            std::process::exit(1);
        } else {
            std::process::exit(0);
        }
    }
}

fn from_rgba(image: image::RgbaImage) -> imgref::ImgVec<[u8; 4]> {
    // These conversions cannot fail because if they didn't fit in `usize`,
    // the `buf` couldn't exist in memory.
    let width = usize::try_from(image.width()).expect("width too large");
    let height = usize::try_from(image.height()).expect("height too large");

    let buf = image
        .into_vec()
        .chunks(4)
        .map(|pixel| <[u8; 4]>::try_from(pixel).unwrap())
        .collect();

    imgref::ImgVec::new(buf, width, height)
}

fn noop() -> impl FnMut(ResMut<TestTask>) {
    move |mut task| task.finish()
}

fn current_task(id: usize) -> impl FnMut(Res<TestTask>) -> bool {
    move |state| state.current_task_id().eq(&Some(id))
}

fn assets_loaded(pipelines_ready: Res<PipelinesReady>) -> bool {
    pipelines_ready.0
}

fn sleep_done(task: Res<TestTask>, frame: Res<FrameCount>) -> bool {
    frame.0 > task.sleep_target
}

fn should_run_next_task(test: Res<TestTask>) -> bool {
    if !test.current_task_finished {
        return false;
    }
    if test.screenshot_pending {
        return test.screenshot_finished;
    }
    true
}

#[derive(Resource, Default)]
pub struct TestTask {
    tasks: VecDeque<(usize, TaskInfo)>,
    current_task_finished: bool,
    screenshot_pending: bool,
    screenshot_finished: bool,
    sleep_target: u32,
}

impl TestTask {
    fn current_task_id(&self) -> Option<usize> {
        self.tasks.front().map(|info| info.0)
    }

    pub fn finish(&mut self) {
        self.current_task_finished = true;
    }

    fn reset_current_task_state(&mut self) {
        self.screenshot_finished = false;
        self.screenshot_pending = false;
        self.current_task_finished = false;
    }
}

fn run_task(
    mut commands: Commands,
    mut state: ResMut<TestTask>,
    config: Res<ScreenshotTestConfig>,
) {
    if let Some((_, task)) = state.tasks.front() {
        if !task.screenshot.is_empty() && !state.screenshot_pending {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_sc(
                    config.actual_dir.join(format!("{}.png", task.screenshot)),
                ));
            state.screenshot_pending = true;
            return;
        }
    }

    let Some(task) = state.tasks.pop_front() else {
        info!("No tasks added");
        commands.write_message(AppExit::Success);
        return;
    };
    info!("Finished task {}: {}", task.0, task.1.name);
    state.reset_current_task_state();

    if state.tasks.is_empty() {
        info!("end of tasks");
        commands.write_message(AppExit::Success);
        return;
    }
}

fn save_sc(path: impl AsRef<Path>) -> impl FnMut(On<ScreenshotCaptured>, ResMut<TestTask>) {
    let path = path.as_ref().to_owned();
    fs::create_dir_all(path.clone().parent().unwrap()).unwrap_or(());
    fs::remove_file(path.clone()).unwrap_or(());
    move |screenshot_captured, mut task: ResMut<TestTask>| {
        save_to_disk(path.clone())(screenshot_captured);
        task.screenshot_finished = true;
    }
}

mod pipelines_ready {
    use bevy::{
        prelude::*,
        render::{render_resource::*, *},
    };

    pub struct PipelinesReadyPlugin;
    impl Plugin for PipelinesReadyPlugin {
        fn build(&self, app: &mut App) {
            app.insert_resource(PipelinesReady::default());

            // In order to gain access to the pipelines status, we have to
            // go into the `RenderApp`, grab the resource from the main App
            // and then update the pipelines status from there.
            // Writing between these Apps can only be done through the
            // `ExtractSchedule`.
            app.sub_app_mut(RenderApp)
                .add_systems(ExtractSchedule, update_pipelines_ready);
        }
    }

    #[derive(Resource, Debug, Default)]
    pub struct PipelinesReady(pub bool);

    fn update_pipelines_ready(mut main_world: ResMut<MainWorld>, pipelines: Res<PipelineCache>) {
        if let Some(mut pipelines_ready) = main_world.get_resource_mut::<PipelinesReady>() {
            pipelines_ready.0 = pipelines.waiting_pipelines().count() == 0;
        }
    }
}
