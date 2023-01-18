use std::{borrow::Cow, cell::RefCell, fmt::Write, time::Duration};

use anyhow::Result;
use console::style;
use indicatif::{
    MultiProgress, ProgressBar, ProgressDrawTarget, ProgressFinish, ProgressState, ProgressStyle,
};
use once_cell::sync::Lazy;

use crate::config::Config;

static HEADER_IN_PROGRESS_STYLE: Lazy<ProgressStyle> =
    Lazy::new(|| ProgressStyle::with_template("[+] Running {pos}/{len}").unwrap());
static HEADER_FINISHED_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::with_template(&style("[+] Running {pos}/{len}").blue().to_string()).unwrap()
});

static SPINNER_IN_PROGRESS_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::with_template(" {spinner} {prefix}  {wide_msg} {elapsed} ")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "⠿"])
        .with_key("elapsed", |state: &ProgressState, w: &mut dyn Write| {
            write!(w, "{:.1}s", state.elapsed().as_secs_f64()).unwrap();
        })
});
static SPINNER_FINISHED_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    SPINNER_IN_PROGRESS_STYLE
        .clone()
        .template(" {spinner:.blue} {prefix:.blue}  {wide_msg:.blue} {elapsed:.blue} ")
        .unwrap()
});
static SPINNER_ERROR_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    SPINNER_IN_PROGRESS_STYLE
        .clone()
        .template(" {spinner:.red} {prefix:.red}  {wide_msg:.red} {elapsed:.red} ")
        .unwrap()
});

#[derive(Debug)]
pub(crate) struct Progress {
    progress: MultiProgress,
    header: ProgressBar,
    spinners: RefCell<Vec<Spinner>>,
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
                .with_finish(ProgressFinish::Abandon)
                .with_style(HEADER_IN_PROGRESS_STYLE.clone()),
        );

        Self {
            progress,
            header,
            spinners: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn add_spinner(
        &self,
        prefix: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Spinner {
        self.header.inc_length(1);

        let inner = self.progress.add(
            ProgressBar::new(0)
                .with_prefix(prefix)
                .with_message(message)
                .with_finish(ProgressFinish::AbandonWithMessage(Cow::Borrowed("Aborted")))
                .with_style(SPINNER_IN_PROGRESS_STYLE.clone()),
        );
        inner.enable_steady_tick(Duration::from_millis(100));

        self.spinners.borrow_mut().push(Spinner {
            inner: inner.clone(),
            header: self.header.clone(),
        });

        let width = self
            .spinners
            .borrow()
            .iter()
            .map(|spinner| spinner.inner.prefix().trim().len())
            .max()
            .unwrap_or_default();

        for spinner in self.spinners.borrow().iter() {
            spinner
                .inner
                .set_prefix(format!("{:width$}", spinner.inner.prefix().trim()));
        }

        Spinner {
            inner,
            header: self.header.clone(),
        }
    }

    pub(crate) fn finish(&self) {
        self.header.set_style(HEADER_FINISHED_STYLE.clone());
        self.header.finish();
    }
}

#[derive(Debug)]
pub(crate) struct Spinner {
    inner: ProgressBar,
    header: ProgressBar,
}

impl Spinner {
    pub(crate) fn finish_with_message(&self, message: impl Into<Cow<'static, str>>) {
        self.inner.set_style(SPINNER_FINISHED_STYLE.clone());
        self.inner.finish_with_message(message);

        self.header.inc(1);
    }
}

pub(crate) trait Finish {
    fn finish_with_message(self, spinner: Spinner, message: impl Into<Cow<'static, str>>) -> Self;
}

impl<T> Finish for Result<T> {
    fn finish_with_message(self, spinner: Spinner, message: impl Into<Cow<'static, str>>) -> Self {
        if self.is_ok() {
            spinner.finish_with_message(message);
        } else {
            spinner.inner.set_style(SPINNER_ERROR_STYLE.clone());
            spinner.inner.finish_with_message("Error");
        }

        self
    }
}
