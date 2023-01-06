use std::{fmt::Write, time::Duration};

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
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

pub(crate) struct Progress {
    progress: MultiProgress,
    pub(crate) header: ProgressBar,
    spinner_style: ProgressStyle,
}

impl Progress {
    pub(crate) fn new(config: &Config, width: usize) -> Self {
        let progress = MultiProgress::with_draw_target(if config.dry_run {
            ProgressDrawTarget::hidden()
        } else {
            ProgressDrawTarget::stderr()
        });
        let header = progress.add(ProgressBar::new(0).with_style(HEADER_STYLE.clone()));
        let spinner_style = ProgressStyle::with_template(&format!(
            " {{spinner:.blue}} {{prefix:{}.blue}}  {{wide_msg:.blue}} {{elapsed:.blue}} ",
            width
        ))
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "⠿"])
        .with_key("elapsed", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{:.1}s", state.elapsed().as_secs_f64()).unwrap();
        });

        Self {
            progress,
            header,
            spinner_style,
        }
    }

    pub(crate) fn add_spinner(&self) -> ProgressBar {
        let spinner = self
            .progress
            .add(ProgressBar::new(0).with_style(self.spinner_style.clone()));

        spinner.enable_steady_tick(Duration::from_millis(100));

        spinner
    }
}
