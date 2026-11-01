//! Builds a report of the latest analysis, and renders it as one HTML file.
//!
//! The file runs no script and loads nothing, so it can be opened anywhere. Paths
//! and commit subjects come from the repository, so every value is escaped here.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_model::{
    APP_VERSION, AtlasConfig, AtlasError, BACKTEST_VERSION, Estimate, GraphNode, Ranker, RepoPath,
    ReportArchitecture, ReportBacktest, ReportDocument, ReportProvenance, RepositoryEntry,
    TimelinePoint,
};
use atlas_storage::Database;

use crate::graph_view::load;
use crate::overview::overview;
use crate::timeline::{DEFAULT_TIMELINE_POINTS, timeline};

const LISTED_CYCLES: usize = 10;
const LISTED_IMPORTED: usize = 10;

const LIMITATIONS: [&str; 5] = [
    "Imports are read from source. An import says what the code declares, not what runs.",
    "Co-change is correlation. Files can change together for unrelated reasons.",
    "Only first-parent history inside the analyzed window is counted, and a merge counts as one \
     change set.",
    "Only TypeScript and TSX are analyzed. Other files count towards history but get no analysis.",
    "A backtest measures which files change together in later commits. It does not measure bugs, \
     causes or productivity.",
];

/// Collects a report of the latest analysis. It reads the newest backtest only if
/// that ran on the same commit, so the sections always describe one state.
pub fn report(
    database: &Database,
    repository: &RepositoryEntry,
    config: &AtlasConfig,
) -> Result<ReportDocument, AtlasError> {
    let overview = overview(database, repository.id)?;
    let run = overview.run;

    // A repository over a graph limit keeps its history analysis, so its report
    // still has that part.
    let (architecture, points) = match load(database, repository, None, config) {
        Ok(snapshot) => {
            let coverage = database.snapshot_coverage(snapshot.id)?;
            let architecture = snapshot.architecture;
            let cyclic: BTreeSet<&RepoPath> = architecture
                .cycles
                .iter()
                .flat_map(|cycle| cycle.members.iter())
                .collect();
            let mut most_imported: Vec<GraphNode> = architecture
                .nodes
                .iter()
                .filter(|node| node.in_degree > 0)
                .cloned()
                .collect();
            most_imported.sort_by(|a, b| {
                b.in_degree
                    .cmp(&a.in_degree)
                    .then_with(|| a.path.cmp(&b.path))
            });
            most_imported.truncate(LISTED_IMPORTED);
            let summary = ReportArchitecture {
                file_count: architecture.nodes.len() as u32,
                import_count: architecture.edges.len() as u32,
                cycle_count: architecture.cycles.len() as u32,
                cyclic_file_count: cyclic.len() as u32,
                largest_cycles: architecture
                    .cycles
                    .iter()
                    .take(LISTED_CYCLES)
                    .cloned()
                    .collect(),
                most_imported,
                coverage,
            };
            let points = timeline(database, repository, DEFAULT_TIMELINE_POINTS, config)?;
            (Some(summary), points.points)
        }
        Err(AtlasError::TooLarge(_)) => (None, Vec::new()),
        Err(error) => return Err(error),
    };

    let backtest = database
        .latest_backtest(repository.id)?
        .map(|entry| entry.result)
        .filter(|result| result.target_oid == run.target_oid)
        .map(|result| ReportBacktest {
            warmup: result.warmup,
            coverage: result.coverage,
            counts: result.counts,
            rankers: result.rankers,
            notes: result.notes,
        });

    let repository_name = Path::new(&repository.path)
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Ok(ReportDocument {
        provenance: ReportProvenance {
            tool_version: APP_VERSION.to_owned(),
            generated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs() as i64),
            repository_name,
            commit: run.target_oid,
            history_limit: run.history_limit,
            parser_version: run.parser_version,
            algorithm_version: run.algorithm_version,
            backtest_version: BACKTEST_VERSION,
            config_hash: run.config_hash,
        },
        history: overview.history,
        files: overview.files,
        exclusions: overview.exclusions,
        top_churn: overview.top_churn,
        architecture,
        timeline: points,
        backtest,
        limitations: LIMITATIONS.map(str::to_owned).to_vec(),
    })
}

/// Escapes text for HTML content and for quoted attributes.
fn esc(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c if c.is_control() => escaped.push('\u{FFFD}'),
            c => escaped.push(c),
        }
    }
    escaped
}

/// A UTC date, from days since 1970 (Hinnant's civil-from-days).
fn date(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn ranker_label(ranker: Ranker) -> &'static str {
    match ranker {
        Ranker::Combined => "Change Impact",
        Ranker::CoChange => "Co-change alone",
        Ranker::ReverseDependency => "Imports alone",
        Ranker::MostChanged => "Most changed files",
        Ranker::Random => "Random",
    }
}

fn percent(estimate: &Estimate) -> String {
    format!(
        "{:.1}% <small>({:.1}–{:.1})</small>",
        estimate.mean * 100.0,
        estimate.low * 100.0,
        estimate.high * 100.0
    )
}

const STYLE: &str = "\
:root{color-scheme:light dark;--ink:#1e293b;--soft:#526075;--line:#dbe1e9;--bg:#fff;--series:#2a78d6}\
@media(prefers-color-scheme:dark){:root{--ink:#e2e8f0;--soft:#a3afc0;--line:#333c49;--bg:#14181f;--series:#3987e5}}\
body{margin:0 auto;max-width:860px;padding:32px 20px;background:var(--bg);color:var(--ink);\
font:15px/1.5 system-ui,-apple-system,'Segoe UI',sans-serif}\
h1{font-size:24px;margin:0}h2{font-size:17px;margin:32px 0 8px}\
.soft,small{color:var(--soft)}code,td.path{font-family:ui-monospace,Menlo,monospace;font-size:13px}\
table{border-collapse:collapse;width:100%}th,td{padding:4px 8px;border-bottom:1px solid var(--line);text-align:left}\
th{font-weight:600;color:var(--soft);font-size:13px}td.n,th.n{text-align:right;font-variant-numeric:tabular-nums}\
td.path{word-break:break-all}\
.facts{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin:16px 0}\
.facts b{display:block;font-size:22px;font-weight:600}\
svg{display:block;width:100%;height:auto}svg text{fill:var(--soft);font-size:11px}\
polyline{fill:none;stroke:var(--series);stroke-width:2}circle{fill:var(--series)}\
.note{border-left:3px solid var(--line);padding-left:12px}";

fn fact(out: &mut String, label: &str, value: u32) {
    let _ = write!(
        out,
        "<div><b>{value}</b><span class=\"soft\">{label}</span></div>"
    );
}

/// A line of one metric over commits. A plain `title` on each point stands in for
/// a tooltip, as the file has no script, and the table below has the same values.
fn line_chart(
    out: &mut String,
    label: &str,
    points: &[TimelinePoint],
    value: fn(&TimelinePoint) -> u32,
) {
    const WIDTH: f64 = 800.0;
    const HEIGHT: f64 = 90.0;
    const LEFT: f64 = 44.0;
    const TOP: f64 = 8.0;
    let top = points.iter().map(value).max().unwrap_or(0);
    let x = |index: usize| {
        LEFT + (WIDTH - LEFT - 8.0) * index as f64 / (points.len().max(2) - 1) as f64
    };
    let y = |metric: u32| {
        let share = if top == 0 {
            0.0
        } else {
            f64::from(metric) / f64::from(top)
        };
        TOP + (HEIGHT - TOP - 8.0) * (1.0 - share)
    };
    let line: Vec<String> = points
        .iter()
        .enumerate()
        .map(|(index, point)| format!("{:.1},{:.1}", x(index), y(value(point))))
        .collect();
    let _ = write!(
        out,
        "<h3>{label}</h3><svg viewBox=\"0 0 {WIDTH} {HEIGHT}\" role=\"img\" aria-label=\"{label} \
         by commit, up to {top}. The table below has the values.\">\
         <text x=\"0\" y=\"{}\">{top}</text><text x=\"0\" y=\"{}\">0</text>\
         <polyline points=\"{}\"/>",
        TOP + 8.0,
        HEIGHT - 8.0,
        line.join(" ")
    );
    for (index, point) in points.iter().enumerate() {
        let _ = write!(
            out,
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.5\"><title>{}: {}</title></circle>",
            x(index),
            y(value(point)),
            esc(&point.commit.oid.as_str()[..10]),
            value(point)
        );
    }
    out.push_str("</svg>");
}

fn timeline_section(out: &mut String, points: &[TimelinePoint]) {
    if points.len() < 2 {
        return;
    }
    out.push_str("<h2>How the import graph changed</h2>");
    line_chart(out, "Source files", points, |point| point.file_count);
    line_chart(out, "Imports", points, |point| point.import_count);
    line_chart(out, "Import cycles", points, |point| point.cycle_count);
    out.push_str(
        "<table><tr><th>Commit</th><th>Subject</th><th class=n>Files</th><th class=n>Imports</th>\
         <th class=n>Cycles</th></tr>",
    );
    for point in points {
        let _ = write!(
            out,
            "<tr><td><code>{}</code></td><td>{}</td><td class=n>{}</td><td class=n>{}</td>\
             <td class=n>{}</td></tr>",
            esc(&point.commit.oid.as_str()[..10]),
            esc(&point.commit.subject),
            point.file_count,
            point.import_count,
            point.cycle_count
        );
    }
    out.push_str("</table>");
}

fn architecture_section(out: &mut String, architecture: &ReportArchitecture) {
    out.push_str("<h2>Import graph</h2><div class=facts>");
    fact(out, "source files", architecture.file_count);
    fact(out, "imports", architecture.import_count);
    fact(out, "import cycles", architecture.cycle_count);
    fact(out, "files in a cycle", architecture.cyclic_file_count);
    out.push_str("</div>");

    let coverage = &architecture.coverage;
    let _ = write!(
        out,
        "<p class=soft>{} files parsed, {} of them with syntax errors, {} skipped. {} imports \
         resolved, {} external, {} unresolved.</p>",
        coverage.parsed_file_count,
        coverage.partial_file_count,
        coverage.skipped_file_count,
        coverage.resolved_import_count,
        coverage.external_import_count,
        coverage.unresolved_import_count
    );

    if !architecture.most_imported.is_empty() {
        out.push_str(
            "<h3>Most imported files</h3><table><tr><th>File</th><th class=n>Imported by</th>\
             <th class=n>Imports</th></tr>",
        );
        for node in &architecture.most_imported {
            let _ = write!(
                out,
                "<tr><td class=path>{}</td><td class=n>{}</td><td class=n>{}</td></tr>",
                esc(node.path.as_str()),
                node.in_degree,
                node.out_degree
            );
        }
        out.push_str("</table>");
    }
    if !architecture.largest_cycles.is_empty() {
        out.push_str(
            "<h3>Largest import cycles</h3><table><tr><th>Files</th><th>Members</th></tr>",
        );
        for cycle in &architecture.largest_cycles {
            let members: Vec<String> = cycle.members.iter().map(|m| esc(m.as_str())).collect();
            let kind = if cycle.type_only {
                " <small>types only</small>"
            } else {
                ""
            };
            let _ = write!(
                out,
                "<tr><td class=n>{}{kind}</td><td class=path>{}</td></tr>",
                cycle.members.len(),
                members.join("<br>")
            );
        }
        out.push_str("</table>");
    }
}

fn backtest_section(out: &mut String, backtest: &ReportBacktest) {
    let _ = write!(
        out,
        "<h2>Backtest</h2><p class=note>This measures which files change together in later \
         commits. It does not measure bugs, causes or productivity.</p>\
         <p class=soft>{} commits evaluated after a warm-up of {}; {:.0}% of candidate commits \
         could be evaluated. Brackets show bootstrap 95% intervals.</p>",
        backtest.counts.evaluated_cases,
        backtest.warmup,
        backtest.coverage * 100.0
    );
    out.push_str(
        "<table><tr><th>Ranking</th><th class=n>Files</th><th class=n>Hit rate</th>\
         <th class=n>Recall</th><th class=n>Precision</th></tr>",
    );
    for ranker in &backtest.rankers {
        for metrics in &ranker.metrics {
            let _ = write!(
                out,
                "<tr><td>{}</td><td class=n>{}</td><td class=n>{}</td><td class=n>{}</td>\
                 <td class=n>{}</td></tr>",
                ranker_label(ranker.ranker),
                metrics.k,
                percent(&metrics.hit_rate),
                percent(&metrics.recall),
                percent(&metrics.precision)
            );
        }
    }
    out.push_str("</table><ul>");
    for note in &backtest.notes {
        let _ = write!(out, "<li>{}</li>", esc(note));
    }
    out.push_str("</ul>");
}

/// Renders a report as a self-contained page.
pub fn render_report(report: &ReportDocument) -> String {
    let provenance = &report.provenance;
    let name = esc(&provenance.repository_name);
    let mut out = String::new();
    let _ = write!(
        out,
        "<!DOCTYPE html><html lang=en><head><meta charset=utf-8>\
         <meta http-equiv=Content-Security-Policy content=\"default-src 'none'; \
         style-src 'unsafe-inline'\">\
         <meta name=viewport content=\"width=device-width,initial-scale=1\">\
         <title>CodeAtlas report: {name}</title><style>{STYLE}</style></head><body>\
         <h1>{name}</h1><p class=soft>CodeAtlas report, commit <code>{}</code>, generated {}</p>",
        esc(&provenance.commit.as_str()[..10]),
        date(provenance.generated_at)
    );

    out.push_str("<h2>History</h2><div class=facts>");
    fact(
        &mut out,
        "first-parent commits",
        report.history.commit_count,
    );
    fact(&mut out, "merges", report.history.merge_commit_count);
    fact(
        &mut out,
        "files at this commit",
        report.files.tip_file_count,
    );
    fact(
        &mut out,
        "TS and TSX files",
        report.files.tip_typescript_file_count,
    );
    fact(&mut out, "renames", report.files.rename_count);
    out.push_str("</div>");
    if report.history.is_shallow {
        out.push_str("<p class=note>This is a shallow clone, so older history is missing.</p>");
    }
    if report.history.is_truncated {
        let _ = write!(
            out,
            "<p class=note>The history limit of {} cut off older commits.</p>",
            provenance.history_limit
        );
    }

    if !report.top_churn.is_empty() {
        out.push_str(
            "<h3>Most changed files</h3><table><tr><th>File</th><th class=n>Commits</th>\
             <th class=n>Lines added</th><th class=n>Lines deleted</th></tr>",
        );
        for file in &report.top_churn {
            let deleted = if file.exists_at_tip {
                ""
            } else {
                " <small>deleted</small>"
            };
            let _ = write!(
                out,
                "<tr><td class=path>{}{deleted}</td><td class=n>{}</td><td class=n>{}</td>\
                 <td class=n>{}</td></tr>",
                esc(file.path.as_str()),
                file.commit_count,
                file.added_lines,
                file.deleted_lines
            );
        }
        out.push_str("</table>");
    }

    match &report.architecture {
        Some(architecture) => architecture_section(&mut out, architecture),
        None => out.push_str(
            "<h2>Import graph</h2><p class=note>This repository is over a graph limit, so no \
             import graph was built.</p>",
        ),
    }
    timeline_section(&mut out, &report.timeline);
    if let Some(backtest) = &report.backtest {
        backtest_section(&mut out, backtest);
    }

    out.push_str("<h2>What these figures do not show</h2><ul>");
    for limitation in &report.limitations {
        let _ = write!(out, "<li>{}</li>", esc(limitation));
    }
    let _ = write!(
        out,
        "</ul><h2>Provenance</h2><table>\
         <tr><th>CodeAtlas</th><td>{}</td></tr>\
         <tr><th>Commit</th><td><code>{}</code></td></tr>\
         <tr><th>History limit</th><td>{}</td></tr>\
         <tr><th>Parser version</th><td>{}</td></tr>\
         <tr><th>Algorithm version</th><td>{}</td></tr>\
         <tr><th>Backtest version</th><td>{}</td></tr>\
         <tr><th>Settings hash</th><td><code>{}</code></td></tr></table></body></html>",
        esc(&provenance.tool_version),
        esc(provenance.commit.as_str()),
        provenance.history_limit,
        provenance.parser_version,
        provenance.algorithm_version,
        provenance.backtest_version,
        esc(&provenance.config_hash)
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{date, esc};

    #[test]
    fn escaping_covers_every_character_that_can_end_a_context() {
        assert_eq!(
            esc("<a href=\"x\" id='y'>&\u{7}"),
            "&lt;a href=&quot;x&quot; id=&#39;y&#39;&gt;&amp;\u{FFFD}"
        );
    }

    #[test]
    fn dates_are_utc_calendar_days() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_704_067_200), "2024-01-01");
        assert_eq!(date(1_735_689_599), "2024-12-31");
    }
}
