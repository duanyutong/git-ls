use crate::model::RewrittenCommit;

use super::context::RenderContext;
use super::metadata::format_age;

pub(super) fn display_rewritten_commit(
    commit: &RewrittenCommit,
    ctx: &RenderContext<'_>,
) -> String {
    let old_oid = ctx.colours.metadata_oid(&commit.meta.short_oid);
    let replacement_oid = ctx.colours.metadata_oid(&commit.replacement.short_oid);

    if !ctx.verbosity.includes_metadata() {
        return format!("{old_oid} rewritten as {replacement_oid}");
    }

    let age = format_age(ctx.now_timestamp, commit.meta.timestamp);
    let age = ctx.colours.metadata_age(&format!(
        "{age:>age_width$}",
        age_width = ctx.metadata_widths.age
    ));
    let open = ctx.colours.metadata_punctuation("(");
    let status = ctx.colours.metadata_count("rewritten as");
    let close = ctx.colours.metadata_punctuation(")");
    let prefix = format!("{age} {open}{status} {replacement_oid}{close}");

    if ctx.verbosity.includes_title() {
        format!(
            "{prefix} {old_oid} {}",
            ctx.colours.commit_title(&commit.meta.subject)
        )
    } else {
        format!("{prefix} {old_oid}")
    }
}
