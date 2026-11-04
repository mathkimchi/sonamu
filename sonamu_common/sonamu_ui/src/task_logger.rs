//! TODO: move this somewhere else

use std::time::Instant;

pub struct TaskTimer {
    start_time: Instant,
    task_name: String,
}
impl TaskTimer {
    /// task name not unique
    pub fn start(task_name: &impl ToString) -> Self {
        let task_name = task_name.to_string();

        println!("Starting '{task_name}'...");

        Self {
            task_name,
            start_time: Instant::now(),
        }
    }

    pub fn end(self) {
        println!(
            "Finished '{}' in {:?}.",
            self.task_name,
            self.start_time.elapsed()
        );
    }
}

pub fn do_task<O, F: FnOnce() -> O>(task_name: &impl ToString, task_doer: F) -> O {
    let task_timer = TaskTimer::start(task_name);
    let output = task_doer();
    task_timer.end();
    output
}
