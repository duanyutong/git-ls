//! Analysis notifications, independent of clocks and output transports.
use std::fmt;

pub(crate) trait AnalysisObserver {
    fn begin_stage(&self, name: &'static str);
    fn end_stage(&self);
    fn detail(&self, fields: fmt::Arguments<'_>);
    fn progress(&self, message: fmt::Arguments<'_>);
    fn event(&self, fields: fmt::Arguments<'_>);
    fn count(&self, name: &'static str, amount: usize);
}

pub(crate) struct AnalysisStage<'a> {
    observer: &'a dyn AnalysisObserver,
}

impl<'a> AnalysisStage<'a> {
    pub(crate) fn new(observer: &'a dyn AnalysisObserver, name: &'static str) -> Self {
        observer.begin_stage(name);
        Self { observer }
    }

    pub(crate) fn detail(&self, fields: fmt::Arguments<'_>) {
        self.observer.detail(fields);
    }
}

impl Drop for AnalysisStage<'_> {
    fn drop(&mut self) {
        self.observer.end_stage();
    }
}

#[cfg(test)]
impl AnalysisObserver for () {
    fn begin_stage(&self, _name: &'static str) {}
    fn end_stage(&self) {}
    fn detail(&self, _fields: fmt::Arguments<'_>) {}
    fn progress(&self, _message: fmt::Arguments<'_>) {}
    fn event(&self, _fields: fmt::Arguments<'_>) {}
    fn count(&self, _name: &'static str, _amount: usize) {}
}
