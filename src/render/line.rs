use std::ops::Deref;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderLine {
    text: String,
    fixed_suffix: Option<FixedSuffix>,
    diff_stat: Option<DiffStatFallback>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffStatFallback {
    original_suffix_len: Option<usize>,
    minimum_width: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixedSuffix {
    prefix: String,
    suffix: String,
}

impl RenderLine {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fixed_suffix: None,
            diff_stat: None,
        }
    }

    pub(crate) fn with_fixed_suffix(prefix: impl Into<String>, suffix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        let suffix = suffix.into();
        let text = if prefix.is_empty() {
            suffix.clone()
        } else {
            format!("{prefix} {suffix}")
        };
        Self {
            text,
            fixed_suffix: Some(FixedSuffix { prefix, suffix }),
            diff_stat: None,
        }
    }

    pub(crate) fn with_trailing_fixed_suffix(text: String, suffix: String) -> Self {
        let marker = format!(" {suffix}");
        // The fallback moves text whilst the matching arm borrows it.
        #[allow(clippy::option_if_let_else)]
        if let Some(prefix) = text.strip_suffix(&marker) {
            Self::with_fixed_suffix(prefix.to_string(), suffix)
        } else {
            Self::plain(text)
        }
    }

    pub(crate) fn fixed_suffix(&self) -> Option<(&str, &str)> {
        self.fixed_suffix
            .as_ref()
            .map(|fixed| (fixed.prefix.as_str(), fixed.suffix.as_str()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(super) fn with_diff_stat(mut self, stats: &str, minimum_prefix_width: usize) -> Self {
        let original_suffix_len = self.fixed_suffix.as_ref().map(|fixed| fixed.suffix.len());
        if let Some(fixed) = &mut self.fixed_suffix {
            let insertion = format!("{stats} ");
            fixed.suffix.insert_str(0, &insertion);
            let position = if fixed.prefix.is_empty() {
                0
            } else {
                fixed.prefix.len() + 1
            };
            self.text.insert_str(position, &insertion);
        } else {
            self = Self::with_fixed_suffix(self.text, stats);
        }
        let suffix_width = self
            .fixed_suffix
            .as_ref()
            .map_or(0, |fixed| console::measure_text_width(&fixed.suffix));
        self.diff_stat = Some(DiffStatFallback {
            original_suffix_len,
            minimum_width: minimum_prefix_width + 1 + suffix_width,
        });
        self
    }

    pub(crate) fn diff_stat_minimum_width(&self) -> Option<usize> {
        self.diff_stat.as_ref().map(|stat| stat.minimum_width)
    }

    pub(super) fn hide_diff_stat(&mut self) {
        if let Some(stat) = self.diff_stat.take()
            && let Some(mut fixed) = self.fixed_suffix.take()
        {
            if let Some(length) = stat.original_suffix_len {
                fixed.suffix.drain(..fixed.suffix.len() - length);
                self.text.clear();
                self.text.push_str(&fixed.prefix);
                if !fixed.prefix.is_empty() {
                    self.text.push(' ');
                }
                self.text.push_str(&fixed.suffix);
                self.fixed_suffix = Some(fixed);
            } else {
                self.text = fixed.prefix;
            }
        }
    }
}

impl Deref for RenderLine {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<String> for RenderLine {
    fn from(text: String) -> Self {
        Self::plain(text)
    }
}

impl From<&str> for RenderLine {
    fn from(text: &str) -> Self {
        Self::plain(text)
    }
}

impl PartialEq<String> for RenderLine {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<RenderLine> for String {
    fn eq(&self, other: &RenderLine) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<&str> for RenderLine {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<RenderLine> for &str {
    fn eq(&self, other: &RenderLine) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<str> for RenderLine {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
