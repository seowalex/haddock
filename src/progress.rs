use std::{fmt::Write, time::Duration};

use console::style;
use indicatif::{
    MultiProgress, ProgressBar, ProgressDrawTarget, ProgressFinish, ProgressState, ProgressStyle,
};
use once_cell::sync::Lazy;

use crate::config::Config;

static HEADER_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::with_template(&format!(
        "{} {{pos:.blue}}{}{{len:.blue}}",
        style("[+] Running").blue(),
        style("/").blue()
    ))
    .unwrap()
});
static SPINNER_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::with_template(
        " {spinner:.blue} {prefix:.blue} {wide_msg:.blue} {elapsed:.blue} ",
    )
    .unwrap()
    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "⠿"])
    .with_key("elapsed", |state: &ProgressState, w: &mut dyn Write| {
        write!(w, "{:.1}s", state.elapsed().as_secs_f64()).unwrap()
    })
});

#[derive(Debug)]
pub(crate) struct Progress {
    progress: MultiProgress,
    pub(crate) header: ProgressBar,
}

impl Progress {
    pub(crate) fn new(config: &Config) -> Self {
        let progress = MultiProgress::with_draw_target(if config.dry_run {
            ProgressDrawTarget::hidden()
        } else {
            ProgressDrawTarget::stderr()
        });
        let header = progress.add(
            ProgressBar::new(0)
                .with_style(HEADER_STYLE.clone())
                .with_finish(ProgressFinish::AndLeave),
        );

        Self { progress, header }
    }

    pub(crate) fn add_spinner(&self) -> ProgressBar {
        let spinner = self
            .progress
            .add(ProgressBar::new(0).with_style(SPINNER_STYLE.clone()));

        spinner.enable_steady_tick(Duration::from_millis(100));

        spinner
    }
}
