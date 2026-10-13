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
mod tests {
    use super::*;

    #[test]
    fn analysis_adapter_preserves_attribution_and_disabled_recording() {
        use crate::lanes::AnalysisObserver;
        use crate::lanes::AnalysisStage;
        let observer = AnalysisDiagnostics::new(false);
        {
            let span = AnalysisStage::new(&observer, "disabled_analysis");
            span.detail(format_args!("ignored=true"));
            observer.progress(format_args!("ignored=true"));
            observer.event(format_args!("ignored=true"));
            observer.count("ignored", 1);
        }
        assert_eq!(observer.stages.borrow().len(), 0);
        observe(true, || {
            let muted = AnalysisDiagnostics::new(false);
            {
                let stage = AnalysisStage::new(&muted, "muted");
                stage.detail(format_args!("ignored=true"));
                muted.progress(format_args!("ignored=true"));
                muted.event(format_args!("ignored=true"));
                muted.count("ignored", 1);
            }
            RECORDER.with_borrow(|slot| {
                let recorder = slot.as_ref().unwrap();
                assert_eq!(recorder.counters.len(), 0);
                assert_eq!(recorder.timings.len(), 0);
            });
            let observer = AnalysisDiagnostics::new(true);
            observer.detail(format_args!("no_stage=true"));
            {
                let stage = AnalysisStage::new(&observer, "analysis_outer");
                observer.progress(format_args!("analysis test"));
                observer.event(format_args!("test_event=true"));
                {
                    let _inner = AnalysisStage::new(&observer, "analysis_inner");
                    observer.count("vertices", 3);
                }
                stage.detail(format_args!("completed=true"));
            }
            assert_eq!(observer.stages.borrow().len(), 0);
            RECORDER.with_borrow(|slot| {
                let recorder = slot.as_ref().unwrap();
                assert_eq!(recorder.counters[&("analysis_inner", "vertices")], 3);
                assert_eq!(recorder.stack.len(), 0);
            });
            Ok::<_, ()>(())
        })
        .unwrap();
    }

    #[test]
    fn escaped_scopes_and_nested_observation_do_not_corrupt_recording() {
        assert!(!enabled());
        maximum("disabled", 1);
        let span = observe(true, || {
            assert!(enabled());
            maximum("peak", 2);
            maximum("peak", 1);
            Ok::<_, ()>(stage("escaped"))
        })
        .unwrap();
        drop(span);
        observe(true, || observe(true, || Ok::<_, ()>(()))).unwrap();
    }

    #[test]
    fn observation_handles_each_completion_state_without_masking_results() {
        for mode in 0..4 {
            let result = observe(mode != 0, || {
                if mode == 2 {
                    RECORDER.with_borrow_mut(Option::take);
                }
                if mode == 3 { Err(()) } else { Ok(()) }
            });
            assert_eq!(result.is_err(), mode == 3);
        }
        let mut output = Vec::new();
        report(Recorder::default(), Duration::ZERO, true, &mut output);
        assert!(String::from_utf8(output).unwrap().contains("status=ok"));
    }

    #[test]
    fn disabled_diagnostics_have_no_clock_or_recording_state() {
        observe(false, || {
            let mut span = stage("unused");
            span.detail(format_args!("ignored=true"));
            count("unused", 10);
            event(format_args!("ignored=true"));
            assert!(span.active.is_none());
            assert!(span.detail.is_none());
            RECORDER.with_borrow(|slot| assert!(slot.is_none()));
            Ok::<_, ()>(())
        })
        .unwrap();
    }

    #[test]
    fn nested_scopes_attribute_calls_and_counters_without_double_counting() {
        RECORDER.with_borrow_mut(|slot| *slot = Some(Recorder::default()));
        {
            let _outer = stage("selection");
            let _inner = stage("heads");
            count("tips", 3);
            for _ in 0..2 {
                let _operation = operation("merge_base");
            }
        }
        let recorder = RECORDER.with_borrow_mut(Option::take).unwrap();
        assert_eq!(recorder.counters[&("heads", "tips")], 3);
        assert_eq!(
            recorder.timings[&Key {
                kind: "operation",
                name: "merge_base",
                parent: "heads"
            }]
                .calls,
            2
        );
        let mut output = Vec::new();
        report(recorder, Duration::from_millis(12), false, &mut output);
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("stage=heads parent=selection"));
        assert!(output.contains("status=error observed_total_ms=12.000"));
        assert!(output.contains("inclusive=true"));
    }

    #[test]
    fn observation_preserves_results_and_cleans_up_after_errors() {
        let result = observe(true, || {
            let mut span = stage("failing");
            span.detail(format_args!("head=example"));
            event(format_args!("state=test"));
            Err::<(), _>("failure")
        });
        assert_eq!(result, Err("failure"));
        RECORDER.with_borrow(|slot| assert!(slot.is_none()));
        assert_eq!(observe(true, || Ok::<_, ()>(42)), Ok(42));
    }
}
