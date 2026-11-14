//! Fork: the Home dashboard's flow report. How many issues each actor
//! closes per day, and where a closed issue spent its time on the way.
//!
//! Sources, all read-only:
//! - `status_transitions` (migration 046), the dense status history kept
//!   for exactly this question, with the actor of every move;
//! - the audit log's `assign`/`unassign` rows (marked for a person, or
//!   assigned to one) and `wait`/`unwait` rows on a user, which together
//!   say when an issue was in a person's hands;
//! - comment creations, to tell a close that came with a word of evidence
//!   from a silent one;
//! - `issues`, `issue_assignees` and `issue_waits` for what is true now.
//!
//! "Agent" is a bot account (`users.is_bot`), as on the rest of Home.
//! Closures are counted per event: an issue closed, reopened and closed
//! again counts twice, and the reopen shows as rework against the actor
//! who closed it first. Time in stage is measured per issue, once, up to
//! its latest close inside the window.
//!
//! Days are the caller's local days, as in `queries::home`.

use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDateTime};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::error::LificError;

pub const DEFAULT_DAYS: i64 = 30;
pub const MIN_DAYS: i64 = 7;
pub const MAX_DAYS: i64 = 365;
const MAX_TZ_MINUTES: i32 = 14 * 60;
/// A comment by the closer this long before the close, or
/// [`EVIDENCE_AFTER`] after it, counts as evidence for it.
const EVIDENCE_BEFORE: i64 = 60 * 60;
const EVIDENCE_AFTER: i64 = 15 * 60;
/// Rows in each "look at these" list.
const LIST_CAP: usize = 6;
/// Statuses an issue moves through before it is done.
pub const STAGES: [&str; 3] = ["backlog", "todo", "active"];
/// Lead-time histogram: key and exclusive upper bound in seconds.
const SPANS: [(&str, Option<i64>); 7] = [
    ("1h", Some(3_600)),
    ("4h", Some(4 * 3_600)),
    ("1d", Some(86_400)),
    ("3d", Some(3 * 86_400)),
    ("1w", Some(7 * 86_400)),
    ("4w", Some(28 * 86_400)),
    ("more", None),
];

pub struct FlowInput<'a> {
    /// The caller's project filter (`None` = every project).
    pub visible: Option<&'a HashSet<i64>>,
    /// Narrow further to these projects (`None` = no narrowing).
    pub projects: Option<&'a [i64]>,
    /// Window length in local days, ending today.
    pub days: i64,
    pub tz_minutes: i32,
    pub now: NaiveDateTime,
}

#[derive(Debug, Serialize)]
pub struct FlowReport {
    pub days: i64,
    /// First and last local day of the window, `YYYY-MM-DD`.
    pub from: String,
    pub to: String,
    /// Everyone the report names, keyed by `id` from the other sections.
    pub actors: Vec<FlowActor>,
    /// One row per local day of the window, oldest first.
    pub daily: Vec<DayFlow>,
    /// Closes per local day and actor, only the non-zero ones.
    pub closures: Vec<ActorDay>,
    /// One row per actor who closed, cancelled, reopened or holds work.
    pub per_actor: Vec<ActorFlow>,
    pub current: PeriodTotals,
    /// The same totals over the window just before this one.
    pub previous: PeriodTotals,
    /// Time in each of [`STAGES`], plus `people`: the stretch the issue
    /// was marked for or waiting on a person, which overlaps the others.
    pub stages: Vec<StageStat>,
    /// Created → done, over issues closed in the window.
    pub lead: Spread,
    /// First move to active → done; issues that skipped active are left out.
    pub cycle: Spread,
    /// Closed issues whose history the report could read.
    pub timed: i64,
    /// Of those, closed without ever being active.
    pub skipped_active: i64,
    pub lead_histogram: Vec<SpanCount>,
    /// Longest lead times in the window.
    pub slowest: Vec<FlowIssue>,
    /// Active issues, longest in active first.
    pub active: Vec<FlowIssue>,
    pub active_total: i64,
    /// Open issues in a person's hands now, longest waiting first.
    pub waiting: Vec<FlowIssue>,
    pub waiting_total: i64,
    pub waiting_p50: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FlowActor {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub is_bot: bool,
    pub owner_id: Option<i64>,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct DayFlow {
    pub date: String,
    pub done: i64,
    pub cancelled: i64,
    pub reopened: i64,
    pub created_by_agents: i64,
    pub created_by_people: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ActorDay {
    pub date: String,
    /// `None` for system writes (imports, migrations).
    pub actor_id: Option<i64>,
    pub done: i64,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct ActorFlow {
    pub actor_id: Option<i64>,
    pub done: i64,
    pub cancelled: i64,
    /// Reopens in the window of issues this actor had closed.
    pub reopened: i64,
    /// Their closes with a comment of theirs around the close.
    pub with_evidence: i64,
    pub cycle_p50: Option<i64>,
    pub lead_p50: Option<i64>,
    /// Active issues they last moved to active.
    pub active_now: i64,
    pub last_close: Option<String>,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct PeriodTotals {
    pub done: i64,
    pub cancelled: i64,
    pub reopened: i64,
    pub created: i64,
    pub with_evidence: i64,
    pub lead_p50: Option<i64>,
    pub cycle_p50: Option<i64>,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct Spread {
    pub count: i64,
    pub p50: Option<i64>,
    pub p85: Option<i64>,
    pub mean: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct StageStat {
    /// A status from [`STAGES`], or `people`.
    pub stage: &'static str,
    /// Issues that spent any time in it.
    pub issues: i64,
    pub p50: Option<i64>,
    pub p85: Option<i64>,
    /// Seconds summed over every closed issue, for shares.
    pub total: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SpanCount {
    pub key: &'static str,
    pub count: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FlowIssue {
    pub issue_id: i64,
    pub project_id: i64,
    pub identifier: String,
    pub title: String,
    pub status: String,
    /// Seconds: lead time (slowest), time in active (active) or time in a
    /// person's hands (waiting).
    pub seconds: i64,
    /// Who closed it, moved it to active, or asked the person.
    pub actor_id: Option<i64>,
    /// Latest activity on the issue (UTC), for spotting stalled work.
    pub last_activity: Option<String>,
}

struct IssueRow {
    project_id: i64,
    identifier: String,
    title: String,
    status: String,
    created: NaiveDateTime,
}

struct Move {
    from: String,
    to: String,
    actor: Option<i64>,
    at: NaiveDateTime,
}

pub fn flow(conn: &Connection, input: &FlowInput) -> Result<FlowReport, LificError> {
    let tz = input.tz_minutes.clamp(-MAX_TZ_MINUTES, MAX_TZ_MINUTES);
    let offset = Duration::minutes(tz as i64);
    let days = input.days.clamp(MIN_DAYS, MAX_DAYS);
    let today = (input.now + offset).date();
    let first = today - Duration::days(days - 1);
    let start = first.and_hms_opt(0, 0, 0).unwrap() - offset;
    let prev_start = start - Duration::days(days);
    let only: Option<HashSet<i64>> = input.projects.map(|p| p.iter().copied().collect());
    let visible = |pid: Option<i64>| {
        let Some(pid) = pid else { return false };
        input.visible.is_none_or(|set| set.contains(&pid))
            && only.as_ref().is_none_or(|set| set.contains(&pid))
    };
    let local_day = |at: NaiveDateTime| (at + offset).date();

    let issues = load_issues(conn, &visible)?;
    let moves = load_moves(conn, &issues)?;
    let people_spans = people_spans(conn, &issues, input.now)?;
    let comments = load_comments(conn, &issues, prev_start)?;
    let last_activity = last_activity(conn, &visible)?;

    let mut daily: Vec<DayFlow> = (0..days)
        .map(|k| DayFlow {
            date: (first + Duration::days(k)).format("%Y-%m-%d").to_string(),
            ..Default::default()
        })
        .collect();
    let day_index = |at: NaiveDateTime| -> Option<usize> {
        let k = (local_day(at) - first).num_days();
        (0..days).contains(&k).then_some(k as usize)
    };
    let mut closures: HashMap<(usize, Option<i64>), i64> = HashMap::new();
    let mut per_actor: HashMap<Option<i64>, ActorFlow> = HashMap::new();
    let mut actor_cycles: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
    let mut actor_leads: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
    let mut current = PeriodTotals::default();
    let mut previous = PeriodTotals::default();
    let (mut prev_leads, mut prev_cycles) = (Vec::new(), Vec::new());

    // Stage timing, per issue closed in the window.
    let mut stage_times: HashMap<&'static str, Vec<i64>> = HashMap::new();
    let mut stage_totals: HashMap<&'static str, i64> = HashMap::new();
    let (mut leads, mut cycles) = (Vec::new(), Vec::new());
    let mut timed = 0;
    let mut skipped_active = 0;
    let mut histogram = [0i64; SPANS.len()];
    let mut slowest: Vec<FlowIssue> = Vec::new();

    for (issue_id, list) in &moves {
        let issue = &issues[issue_id];
        let mut last_closer: Option<Option<i64>> = None;
        // The latest close in each window, for stage timing.
        let mut final_close: Option<&Move> = None;
        let mut prev_final: Option<&Move> = None;
        for m in list {
            let in_window = m.at >= start;
            let in_prev = !in_window && m.at >= prev_start;
            if m.to == "done" {
                if in_window {
                    let entry = per_actor.entry(m.actor).or_default();
                    entry.done += 1;
                    let ts = m.at.format("%Y-%m-%d %H:%M:%S").to_string();
                    if entry.last_close.as_ref().is_none_or(|t| *t < ts) {
                        entry.last_close = Some(ts);
                    }
                    current.done += 1;
                    if has_evidence(&comments, *issue_id, m) {
                        entry.with_evidence += 1;
                        current.with_evidence += 1;
                    }
                    if let Some(k) = day_index(m.at) {
                        daily[k].done += 1;
                        *closures.entry((k, m.actor)).or_default() += 1;
                    }
                    final_close = Some(m);
                } else if in_prev {
                    previous.done += 1;
                    if has_evidence(&comments, *issue_id, m) {
                        previous.with_evidence += 1;
                    }
                    prev_final = Some(m);
                }
                last_closer = Some(m.actor);
            } else if m.to == "cancelled" {
                if in_window {
                    per_actor.entry(m.actor).or_default().cancelled += 1;
                    current.cancelled += 1;
                    if let Some(k) = day_index(m.at) {
                        daily[k].cancelled += 1;
                    }
                } else if in_prev {
                    previous.cancelled += 1;
                }
            } else if m.from == "done" {
                if in_window {
                    current.reopened += 1;
                    if let Some(k) = day_index(m.at) {
                        daily[k].reopened += 1;
                    }
                    if let Some(closer) = last_closer {
                        per_actor.entry(closer).or_default().reopened += 1;
                    }
                } else if in_prev {
                    previous.reopened += 1;
                }
            }
        }

        if let Some(close) = prev_final
            && let Some(t) = timing(issue, list, close, &[])
        {
            prev_leads.push(t.lead);
            if let Some(c) = t.cycle {
                prev_cycles.push(c);
            }
        }
        let Some(close) = final_close else { continue };
        let spans = people_spans.get(issue_id).map(Vec::as_slice).unwrap_or(&[]);
        let Some(t) = timing(issue, list, close, spans) else {
            continue;
        };
        timed += 1;
        leads.push(t.lead);
        actor_leads.entry(close.actor).or_default().push(t.lead);
        match t.cycle {
            Some(c) => {
                cycles.push(c);
                actor_cycles.entry(close.actor).or_default().push(c);
            }
            None => skipped_active += 1,
        }
        for (stage, secs) in t.stages {
            if secs > 0 {
                stage_times.entry(stage).or_default().push(secs);
                *stage_totals.entry(stage).or_default() += secs;
            }
        }
        let bucket = SPANS
            .iter()
            .position(|(_, bound)| bound.is_none_or(|b| t.lead < b))
            .unwrap_or(SPANS.len() - 1);
        histogram[bucket] += 1;
        slowest.push(flow_issue(*issue_id, issue, t.lead, close.actor, &last_activity));
    }

    // Created per day, split by who opened the issue.
    created_counts(conn, &visible, start, prev_start, &day_index, &mut daily, &mut current, &mut previous)?;

    // What is true now.
    let mut active: Vec<FlowIssue> = Vec::new();
    for (issue_id, issue) in &issues {
        if issue.status != "active" {
            continue;
        }
        let entered = moves
            .get(issue_id)
            .and_then(|list| list.iter().rev().find(|m| m.to == "active"));
        let (since, actor) = match entered {
            Some(m) => (m.at, m.actor),
            None => (issue.created, None),
        };
        per_actor.entry(actor).or_default().active_now += 1;
        let secs = (input.now - since).num_seconds().max(0);
        active.push(flow_issue(*issue_id, issue, secs, actor, &last_activity));
    }
    let active_total = active.len() as i64;
    active.sort_by(|a, b| b.seconds.cmp(&a.seconds));
    active.truncate(LIST_CAP);

    let mut waiting: Vec<FlowIssue> = Vec::new();
    for (issue_id, spans) in &people_spans {
        let issue = &issues[issue_id];
        if matches!(issue.status.as_str(), "done" | "cancelled") {
            continue;
        }
        if let Some(open) = spans.iter().find(|s| s.end.is_none()) {
            let secs = (input.now - open.start).num_seconds().max(0);
            waiting.push(flow_issue(*issue_id, issue, secs, open.by, &last_activity));
        }
    }
    let mut waits: Vec<i64> = waiting.iter().map(|w| w.seconds).collect();
    waits.sort_unstable();
    let waiting_total = waiting.len() as i64;
    waiting.sort_by(|a, b| b.seconds.cmp(&a.seconds));
    waiting.truncate(LIST_CAP);

    slowest.sort_by(|a, b| b.seconds.cmp(&a.seconds));
    slowest.truncate(LIST_CAP);

    current.lead_p50 = percentile(&mut leads.clone(), 50);
    current.cycle_p50 = percentile(&mut cycles.clone(), 50);
    previous.lead_p50 = percentile(&mut prev_leads, 50);
    previous.cycle_p50 = percentile(&mut prev_cycles, 50);

    let stages = STAGES
        .iter()
        .copied()
        .chain(["people"])
        .map(|stage| {
            let mut times = stage_times.remove(stage).unwrap_or_default();
            StageStat {
                stage,
                issues: times.len() as i64,
                p50: percentile(&mut times, 50),
                p85: percentile(&mut times, 85),
                total: stage_totals.get(stage).copied().unwrap_or(0),
            }
        })
        .collect();

    for (actor, row) in per_actor.iter_mut() {
        row.actor_id = *actor;
        row.cycle_p50 = actor_cycles.get_mut(actor).and_then(|v| percentile(v, 50));
        row.lead_p50 = actor_leads.get_mut(actor).and_then(|v| percentile(v, 50));
    }
    let mut per_actor: Vec<ActorFlow> = per_actor.into_values().collect();
    per_actor.sort_by(|a, b| {
        (b.done, b.active_now, b.cancelled)
            .cmp(&(a.done, a.active_now, a.cancelled))
            .then(a.actor_id.cmp(&b.actor_id))
    });

    let mut closures: Vec<ActorDay> = closures
        .into_iter()
        .map(|((k, actor_id), done)| ActorDay {
            date: daily[k].date.clone(),
            actor_id,
            done,
        })
        .collect();
    closures.sort_by(|a, b| a.date.cmp(&b.date).then(a.actor_id.cmp(&b.actor_id)));

    let mut named: HashSet<i64> = per_actor.iter().filter_map(|a| a.actor_id).collect();
    for list in [&active, &waiting, &slowest] {
        named.extend(list.iter().filter_map(|i| i.actor_id));
    }

    Ok(FlowReport {
        days,
        from: first.format("%Y-%m-%d").to_string(),
        to: today.format("%Y-%m-%d").to_string(),
        actors: load_actors(conn, &named)?,
        daily,
        closures,
        per_actor,
        current,
        previous,
        stages,
        lead: spread(&mut leads),
        cycle: spread(&mut cycles),
        timed,
        skipped_active,
        lead_histogram: SPANS
            .iter()
            .zip(histogram)
            .map(|((key, _), count)| SpanCount { key, count })
            .collect(),
        slowest,
        active,
        active_total,
        waiting,
        waiting_total,
        waiting_p50: percentile(&mut waits, 50),
    })
}

fn flow_issue(
    issue_id: i64,
    issue: &IssueRow,
    seconds: i64,
    actor_id: Option<i64>,
    last_activity: &HashMap<i64, String>,
) -> FlowIssue {
    FlowIssue {
        issue_id,
        project_id: issue.project_id,
        identifier: issue.identifier.clone(),
        title: issue.title.clone(),
        status: issue.status.clone(),
        seconds,
        actor_id,
        last_activity: last_activity.get(&issue_id).cloned(),
    }
}

struct Timing {
    lead: i64,
    cycle: Option<i64>,
    stages: Vec<(&'static str, i64)>,
}

/// Walk an issue's history from creation to `close`, summing the time in
/// each stage. `None` when the history cannot be read (the close predates
/// the creation, as an import can leave it).
fn timing(issue: &IssueRow, moves: &[Move], close: &Move, people: &[Span]) -> Option<Timing> {
    let created = issue.created;
    if close.at < created {
        return None;
    }
    let mut status = moves.first().map(|m| m.from.as_str()).unwrap_or("backlog");
    let mut since = created;
    let mut sums: HashMap<&str, i64> = HashMap::new();
    let mut first_active: Option<NaiveDateTime> = (status == "active").then_some(created);
    for m in moves {
        if m.at > close.at || std::ptr::eq(m, close) {
            break;
        }
        let at = m.at.max(since);
        *sums.entry(status).or_default() += (at - since).num_seconds();
        since = at;
        status = m.to.as_str();
        if status == "active" && first_active.is_none() {
            first_active = Some(at);
        }
    }
    *sums.entry(status).or_default() += (close.at - since).num_seconds().max(0);

    let mut stages: Vec<(&'static str, i64)> = STAGES
        .iter()
        .map(|s| (*s, sums.get(s).copied().unwrap_or(0)))
        .collect();
    let with_people: i64 = people
        .iter()
        .map(|s| {
            let a = s.start.max(created);
            let b = s.end.unwrap_or(close.at).min(close.at);
            (b - a).num_seconds().max(0)
        })
        .sum();
    stages.push(("people", with_people));
    Some(Timing {
        lead: (close.at - created).num_seconds(),
        cycle: first_active.map(|a| (close.at - a).num_seconds()),
        stages,
    })
}

fn has_evidence(comments: &HashMap<i64, Vec<(Option<i64>, NaiveDateTime)>>, issue_id: i64, close: &Move) -> bool {
    comments.get(&issue_id).is_some_and(|list| {
        list.iter().any(|(actor, at)| {
            *actor == close.actor
                && *at >= close.at - Duration::seconds(EVIDENCE_BEFORE)
                && *at <= close.at + Duration::seconds(EVIDENCE_AFTER)
        })
    })
}

/// Nearest-rank percentile; sorts `values` in place.
fn percentile(values: &mut [i64], p: usize) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let rank = (p * values.len()).div_ceil(100).max(1);
    Some(values[rank - 1])
}

fn spread(values: &mut [i64]) -> Spread {
    let count = values.len() as i64;
    Spread {
        count,
        p50: percentile(values, 50),
        p85: percentile(values, 85),
        mean: (count > 0).then(|| values.iter().sum::<i64>() / count),
    }
}

fn parse_ts(ts: &str) -> Option<NaiveDateTime> {
    let head = ts.get(..19).unwrap_or(ts).replace('T', " ");
    NaiveDateTime::parse_from_str(&head, "%Y-%m-%d %H:%M:%S").ok()
}

fn load_issues(
    conn: &Connection,
    visible: &dyn Fn(Option<i64>) -> bool,
) -> Result<HashMap<i64, IssueRow>, LificError> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.project_id, p.identifier || '-' || i.sequence, i.title, i.status, i.created_at
           FROM issues i JOIN projects p ON p.id = i.project_id
          WHERE i.deleted_at IS NULL AND NOT EXISTS (SELECT 1 FROM fork_issue_parked fp WHERE fp.issue_id = i.id)",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, project_id, identifier, title, status, created) = row?;
        if !visible(Some(project_id)) {
            continue;
        }
        let Some(created) = parse_ts(&created) else { continue };
        out.insert(
            id,
            IssueRow {
                project_id,
                identifier,
                title,
                status,
                created,
            },
        );
    }
    Ok(out)
}

/// Every visible issue's status moves, oldest first.
fn load_moves(
    conn: &Connection,
    issues: &HashMap<i64, IssueRow>,
) -> Result<HashMap<i64, Vec<Move>>, LificError> {
    let mut stmt = conn.prepare(
        "SELECT issue_id, from_status, to_status, actor_user_id, created_at
           FROM status_transitions ORDER BY issue_id, created_at, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut out: HashMap<i64, Vec<Move>> = HashMap::new();
    for row in rows {
        let (issue_id, from, to, actor, at) = row?;
        if !issues.contains_key(&issue_id) {
            continue;
        }
        let Some(at) = parse_ts(&at) else { continue };
        out.entry(issue_id).or_default().push(Move { from, to, actor, at });
    }
    Ok(out)
}

/// A stretch an issue spent marked for, assigned to, or waiting on a person.
struct Span {
    start: NaiveDateTime,
    end: Option<NaiveDateTime>,
    /// Who put it there.
    by: Option<i64>,
}

/// Rebuilt from the audit log: `assign`/`unassign` of `human` or of a
/// person (`@name` of a non-bot), and `wait`/`unwait` on a user. Overlapping
/// reasons merge into one stretch; a stretch still open has no end.
fn people_spans(
    conn: &Connection,
    issues: &HashMap<i64, IssueRow>,
    now: NaiveDateTime,
) -> Result<HashMap<i64, Vec<Span>>, LificError> {
    let bots: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT username FROM users WHERE is_bot = 1")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<_, _>>()?
    };
    let mut stmt = conn.prepare(
        "SELECT entity_id, action, COALESCE(new_value, old_value), actor_user_id, ts
           FROM audit_log
          WHERE entity_type = 'issue'
            AND (action IN ('assign', 'unassign')
                 OR (action IN ('wait', 'unwait') AND field = 'user'))
          ORDER BY entity_id, ts, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut out: HashMap<i64, Vec<Span>> = HashMap::new();
    let mut holding: HashMap<i64, HashSet<String>> = HashMap::new();
    for row in rows {
        let (issue_id, action, value, actor, at) = row?;
        if !issues.contains_key(&issue_id) {
            continue;
        }
        let (Some(value), Some(at)) = (value, parse_ts(&at)) else {
            continue;
        };
        if action.ends_with("assign") {
            let person = value == "human"
                || value
                    .strip_prefix('@')
                    .is_some_and(|name| !bots.contains(name));
            if !person {
                continue;
            }
        }
        // A wait's value is "@name: note"; one key per reason.
        let key = format!("{}:{value}", if action.ends_with("wait") { "w" } else { "a" });
        let open = holding.entry(issue_id).or_default();
        let spans = out.entry(issue_id).or_default();
        let opening = !action.starts_with("un");
        if opening {
            if open.is_empty() {
                spans.push(Span {
                    start: at,
                    end: None,
                    by: actor,
                });
            }
            open.insert(key);
        } else if open.remove(&key) && open.is_empty()
            && let Some(last) = spans.last_mut()
        {
            last.end = Some(at.min(now));
        }
    }
    // Waits and assignments that predate the audit log still count from
    // when they were made.
    let mut stmt = conn.prepare(
        "SELECT a.issue_id, a.created_at, a.created_by FROM issue_assignees a
           LEFT JOIN users u ON u.id = a.user_id
          WHERE a.user_id IS NULL OR u.is_bot = 0
         UNION ALL
         SELECT w.issue_id, w.created_at, w.created_by FROM issue_waits w WHERE w.kind = 'user'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    for row in rows {
        let (issue_id, at, by) = row?;
        if !issues.contains_key(&issue_id) {
            continue;
        }
        let spans = out.entry(issue_id).or_default();
        if spans.last().is_some_and(|s| s.end.is_none()) {
            continue;
        }
        let Some(at) = parse_ts(&at) else { continue };
        let start = spans.last().and_then(|s| s.end).map_or(at, |end| at.max(end));
        spans.push(Span {
            start,
            end: None,
            by,
        });
    }
    out.retain(|_, spans| !spans.is_empty());
    Ok(out)
}

fn load_comments(
    conn: &Connection,
    issues: &HashMap<i64, IssueRow>,
    floor: NaiveDateTime,
) -> Result<HashMap<i64, Vec<(Option<i64>, NaiveDateTime)>>, LificError> {
    let floor = (floor - Duration::seconds(EVIDENCE_BEFORE))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut stmt = conn.prepare(
        "SELECT issue_id, actor_user_id, ts FROM audit_log
          WHERE entity_type = 'comment' AND action = 'create'
            AND issue_id IS NOT NULL AND ts >= ?1",
    )?;
    let rows = stmt.query_map(params![floor], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut out: HashMap<i64, Vec<(Option<i64>, NaiveDateTime)>> = HashMap::new();
    for row in rows {
        let (issue_id, actor, at) = row?;
        if !issues.contains_key(&issue_id) {
            continue;
        }
        if let Some(at) = parse_ts(&at) {
            out.entry(issue_id).or_default().push((actor, at));
        }
    }
    Ok(out)
}

/// Latest audit entry per issue, its own and its comments'.
fn last_activity(
    conn: &Connection,
    visible: &dyn Fn(Option<i64>) -> bool,
) -> Result<HashMap<i64, String>, LificError> {
    let mut stmt = conn.prepare(
        "SELECT project_id, CASE WHEN entity_type = 'issue' THEN entity_id ELSE issue_id END AS iid,
                MAX(ts)
           FROM audit_log
          WHERE entity_type = 'issue' OR issue_id IS NOT NULL
          GROUP BY iid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (project_id, issue_id, ts) = row?;
        if visible(project_id) {
            out.insert(issue_id, ts);
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn created_counts(
    conn: &Connection,
    visible: &dyn Fn(Option<i64>) -> bool,
    start: NaiveDateTime,
    prev_start: NaiveDateTime,
    day_index: &dyn Fn(NaiveDateTime) -> Option<usize>,
    daily: &mut [DayFlow],
    current: &mut PeriodTotals,
    previous: &mut PeriodTotals,
) -> Result<(), LificError> {
    let mut stmt = conn.prepare(
        "SELECT a.project_id, COALESCE(u.is_bot, 0), a.ts
           FROM audit_log a LEFT JOIN users u ON u.id = a.actor_user_id
          WHERE a.entity_type = 'issue' AND a.action = 'create' AND a.ts >= ?1",
    )?;
    let floor = prev_start.format("%Y-%m-%d %H:%M:%S").to_string();
    let rows = stmt.query_map(params![floor], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, bool>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (project_id, is_bot, at) = row?;
        let Some(at) = parse_ts(&at) else { continue };
        if !visible(project_id) {
            continue;
        }
        if at < start {
            previous.created += 1;
            continue;
        }
        current.created += 1;
        if let Some(k) = day_index(at) {
            if is_bot {
                daily[k].created_by_agents += 1;
            } else {
                daily[k].created_by_people += 1;
            }
        }
    }
    Ok(())
}

fn load_actors(conn: &Connection, ids: &HashSet<i64>) -> Result<Vec<FlowActor>, LificError> {
    let mut stmt = conn.prepare(
        "SELECT id, username, COALESCE(NULLIF(display_name, ''), username), is_bot, owner_id
           FROM users ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(FlowActor {
            id: row.get(0)?,
            username: row.get(1)?,
            display_name: row.get(2)?,
            is_bot: row.get(3)?,
            owner_id: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let actor = row?;
        if ids.contains(&actor.id) {
            out.push(actor);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
