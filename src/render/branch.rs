use console::{measure_text_width, truncate_str};

use crate::model::BranchPoint;

use super::diff::format_diff_stat;
use super::metadata::{branch_metadata_columns, columns_count, format_metadata_prefix};
use super::{RenderContext, RenderLine};

/// Retain field boundaries until width allocation has selected a representation.
pub(super) struct BranchLabel {
    names: Vec<String>,
    current: Option<usize>,
    metadata: String,
    compact_metadata: String,
    status: String,
    subject: String,
    oid: String,
    stats: String,
    compact_stats: String,
    columns: bool,
}

impl BranchLabel {
    pub(super) fn new(
        point: &BranchPoint,
        colour_index: usize,
        orphaned: bool,
        ctx: &RenderContext<'_>,
    ) -> Self {
        let names = point
            .names
            .iter()
            .map(|name| {
                if orphaned {
                    ctx.colours.orphaned_name(name)
                } else if ctx.current_branch == Some(name.as_str()) {
                    ctx.colours.current_stack(colour_index, name)
                } else {
                    ctx.colours.stack(colour_index, name)
                }
            })
            .collect();
        let mut label = Self {
            names,
            current: point
                .names
                .iter()
                .position(|name| ctx.current_branch == Some(name.as_str())),
            metadata: String::new(),
            compact_metadata: String::new(),
            status: if orphaned {
                ctx.colours.orphaned_status("(orphaned)")
            } else {
                String::new()
            },
            subject: String::new(),
            oid: String::new(),
            stats: format_diff_stat(point, ctx, true),
            compact_stats: format_diff_stat(point, ctx, false),
            columns: ctx.layout.is_columns(),
        };
        if let Some(annotation) = point
            .annotation
            .as_ref()
            .filter(|_| ctx.verbosity.includes_metadata())
        {
            let (age, count) = branch_metadata_columns(annotation, ctx.now_timestamp);
            if ctx.verbosity.includes_oid() {
                label.oid = ctx.colours.metadata_oid(&annotation.meta.short_oid);
            }
            if ctx.verbosity.includes_title() {
                label.subject = ctx.colours.commit_title(&annotation.meta.subject);
            }
            label.compact_metadata = if label.columns {
                columns_count(&count, ctx.metadata_widths, ctx.colours, false)
            } else {
                format_metadata_prefix(&age, &count, None, ctx.metadata_widths, ctx.colours)
            };
            label.metadata = if label.columns || label.oid.is_empty() {
                label.compact_metadata.clone()
            } else {
                format_metadata_prefix(
                    &age,
                    &count,
                    Some(&annotation.meta.short_oid),
                    ctx.metadata_widths,
                    ctx.colours,
                )
            };
        }
        label
    }

    pub(super) fn full(&self) -> String {
        join_fields(&[
            &self.metadata,
            &self.names.join(", "),
            &self.status,
            &self.subject,
            if self.columns { &self.oid } else { "" },
        ])
    }

    pub(super) fn render(
        &self,
        prefix: &str,
        continuation: &dyn Fn() -> String,
        width: Option<usize>,
    ) -> Vec<RenderLine> {
        let suffix = join_fields(&[&self.stats, if self.columns { &self.oid } else { "" }]);
        let identity = format!(
            "{prefix}{}",
            join_fields(&[&self.metadata, &self.names.join(", "), &self.status])
        );
        let minimum = measure_text_width(&identity) + suffix_cost(&suffix);
        let Some(width) = width else {
            let body = format!("{identity}{}", subject_suffix(&self.subject));
            return vec![protected_line(body, suffix)];
        };
        if width == 0 {
            return vec![RenderLine::plain("")];
        }
        if minimum <= width {
            let available = width.saturating_sub(minimum + 1);
            let subject = if measure_text_width(&self.subject) <= available || available > 3 {
                truncate(&self.subject, available)
            } else {
                String::new()
            };
            return vec![protected_line(
                format!("{identity}{}", subject_suffix(&subject)),
                suffix,
            )];
        }

        let leading = if self.compact_metadata.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}{} ", self.compact_metadata)
        };
        let available = width.saturating_sub(measure_text_width(&leading));
        let names = self.compact_names(available);
        let identity = format!("{leading}{names}");
        let with_status = format!("{identity}{}", subject_suffix(&self.status));
        let first = if measure_text_width(&with_status) <= width {
            with_status
        } else {
            identity
        };
        let mut lines = vec![RenderLine::plain(truncate(&first, width))];
        let mut suffix = join_fields(&[&self.stats, &self.oid]);
        if suffix.is_empty() {
            return lines;
        }
        if measure_text_width(&suffix) > width {
            suffix = join_fields(&[&self.compact_stats, &self.oid]);
        }
        let available = width.saturating_sub(suffix_cost(&suffix));
        let graph = continuation();
        let graph = truncate_str(&graph, available, "").into_owned();
        lines.push(protected_line(graph, suffix));
        lines
    }

    fn compact_names(&self, width: usize) -> String {
        let mut names: Vec<_> = self.names.iter().map(String::as_str).collect();
        let (label, shown) = select_names(&names, width);
        self.current
            .filter(|index| *index >= shown)
            .map_or(label, |current| {
                let name = names.remove(current);
                names.insert(0, name);
                select_names(&names, width).0
            })
    }
}

fn select_names(names: &[&str], width: usize) -> (String, usize) {
    for shown in (1..=names.len().min(3)).rev() {
        let mut label = names[..shown].join(", ");
        if shown < names.len() {
            label.push_str(", ...");
        }
        if measure_text_width(&label) <= width {
            return (label, shown);
        }
    }
    let first = names.first().copied().unwrap_or_default();
    let remaining = if names.len() > 1 {
        format!("{first}, ...")
    } else {
        first.to_string()
    };
    (truncate(&remaining, width), 0)
}

fn join_fields(fields: &[&str]) -> String {
    fields
        .iter()
        .copied()
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn subject_suffix(subject: &str) -> String {
    if subject.is_empty() {
        String::new()
    } else {
        format!(" {subject}")
    }
}

fn suffix_cost(suffix: &str) -> usize {
    measure_text_width(suffix) + usize::from(!suffix.is_empty())
}

fn protected_line(prefix: String, suffix: String) -> RenderLine {
    if suffix.is_empty() {
        RenderLine::plain(prefix)
    } else {
        RenderLine::with_protected_suffix(prefix, suffix)
    }
}

fn truncate(text: &str, width: usize) -> String {
    truncate_str(text, width, &"..."[..width.min(3)]).into_owned()
}
