//! Invocation-scoped, opt-in performance diagnostics.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::time::{Duration, Instant};

thread_local! {
    static RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Recorder {
    stack: Vec<&'static str>,
    timings: BTreeMap<Key, Timing>,
    counters: BTreeMap<(&'static str, &'static str), usize>,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct Key {
    kind: &'static str,
    name: &'static str,
    parent: &'static str,
}

#[derive(Default)]
struct Timing {
    calls: usize,
    elapsed: Duration,
}

pub(crate) fn enabled() -> bool {
    RECORDER.with(|slot| slot.borrow().is_some())
}

pub(crate) fn maximum(name: &'static str, value: usize) {
    RECORDER.with_borrow_mut(|slot| {
        if let Some(recorder) = slot {
            let parent = recorder.stack.last().copied().unwrap_or("invocation");
            let maximum = recorder.counters.entry((parent, name)).or_default();
            *maximum = (*maximum).max(value);
        }
    });
}

pub(crate) struct Span {
    active: Option<(Instant, Key)>,
    detail: Option<String>,
}

impl Span {
    pub(crate) fn detail(&mut self, fields: fmt::Arguments<'_>) {
        if self.active.is_some() {
            self.detail = Some(fields.to_string());
        }
    }
}

fn begin(kind: &'static str, name: &'static str) -> Span {
    let active = RECORDER.with_borrow_mut(|slot| {
        let recorder = slot.as_mut()?;
        let key = Key {
            kind,
            name,
            parent: recorder.stack.last().copied().unwrap_or("invocation"),
        };
        if kind == "stage" {
            recorder.stack.push(name);
        }
        Some((Instant::now(), key))
    });
    Span {
        active,
        detail: None,
    }
}

pub(crate) fn stage(name: &'static str) -> Span {
    begin("stage", name)
}

pub(crate) fn operation(name: &'static str) -> Span {
    begin("operation", name)
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some((started, key)) = self.active else {
            return;
        };
        let elapsed = started.elapsed();
        RECORDER.with_borrow_mut(|slot| {
            if let Some(recorder) = slot {
                if key.kind == "stage" {
                    recorder.stack.pop();
                }
                let timing = recorder.timings.entry(key).or_default();
                timing.calls += 1;
                timing.elapsed += elapsed;
            }
        });
        if let Some(detail) = &self.detail {
            emit(format_args!(
                "{}={} parent={} duration_ms={:.3} {detail}",
                key.kind,
                key.name,
                key.parent,
                elapsed.as_secs_f64() * 1000.0
            ));
        }
    }
}

pub(crate) fn count(name: &'static str, amount: usize) {
    RECORDER.with_borrow_mut(|slot| {
        if let Some(recorder) = slot {
            let parent = recorder.stack.last().copied().unwrap_or("invocation");
            *recorder.counters.entry((parent, name)).or_default() += amount;
        }
    });
}

pub(crate) fn event(fields: fmt::Arguments<'_>) {
    if RECORDER.with_borrow(Option::is_some) {
        emit(fields);
    }
}

fn emit(fields: fmt::Arguments<'_>) {
    // Diagnostics must not replace a primary error if stderr is unavailable.
    let _ = writeln!(std::io::stderr().lock(), "git-ls debug: perf {fields}");
}

fn report(recorder: Recorder, elapsed: Duration, success: bool, output: &mut impl Write) {
    for (key, timing) in recorder.timings {
        let _ = writeln!(
            output,
            "git-ls debug: perf {}={} parent={} calls={} cumulative_ms={:.3} inclusive=true",
            key.kind,
            key.name,
            key.parent,
            timing.calls,
            timing.elapsed.as_secs_f64() * 1000.0
        );
    }
    for ((parent, name), value) in recorder.counters {
        let _ = writeln!(
            output,
            "git-ls debug: perf counters parent={parent} {name}={value}"
        );
    }
    let status = if success { "ok" } else { "error" };
    let _ = writeln!(
        output,
        "git-ls debug: perf summary status={status} observed_total_ms={:.3} version={}",
        elapsed.as_secs_f64() * 1000.0,
        env!("CARGO_PKG_VERSION")
    );
}

pub(crate) fn observe<T, E>(enabled: bool, run: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    if !enabled {
        return run();
    }
    let started = Instant::now();
    RECORDER.with_borrow_mut(|slot| *slot = Some(Recorder::default()));
    let result = run();
    let elapsed = started.elapsed();
    if let Some(recorder) = RECORDER.with_borrow_mut(Option::take) {
        report(
            recorder,
            elapsed,
            result.is_ok(),
            &mut std::io::stderr().lock(),
        );
    }
    result
}

/// Boundary adapter for analysis notifications. Disabled observers retain no
/// stages; the analysis modules never access clocks or standard streams.
pub(crate) struct AnalysisDiagnostics {
    debug: bool,
    stages: RefCell<Vec<Span>>,
}

impl AnalysisDiagnostics {
    pub(crate) fn new(debug: bool) -> Self {
        Self {
            debug,
            stages: RefCell::default(),
        }
    }
}

impl crate::lanes::AnalysisObserver for AnalysisDiagnostics {
    fn begin_stage(&self, name: &'static str) {
        if self.debug {
            self.stages.borrow_mut().push(stage(name));
        }
    }
    fn end_stage(&self) {
        if self.debug {
            self.stages.borrow_mut().pop();
        }
    }
    fn detail(&self, fields: fmt::Arguments<'_>) {
        if self.debug
            && let Some(span) = self.stages.borrow_mut().last_mut()
        {
            span.detail(fields);
        }
    }
    fn progress(&self, message: fmt::Arguments<'_>) {
        if self.debug {
            let _ = writeln!(std::io::stderr().lock(), "git-ls debug: {message}");
        }
    }
    fn event(&self, fields: fmt::Arguments<'_>) {
        if self.debug {
            event(fields);
        }
    }
    fn count(&self, name: &'static str, amount: usize) {
        if self.debug {
            count(name, amount);
        }
    }
}

#[cfg(test)]
#[path = "tests/diagnostics.rs"]
mod tests;
