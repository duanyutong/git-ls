use std::ops::Deref;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderLine {
    text: String,
    fixed_suffix: Option<FixedSuffix>,
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
        }
    }

    pub(crate) fn with_trailing_fixed_suffix(text: String, suffix: String) -> Self {
        let marker = format!(" {suffix}");
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
