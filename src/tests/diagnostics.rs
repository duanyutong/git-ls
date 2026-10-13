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
