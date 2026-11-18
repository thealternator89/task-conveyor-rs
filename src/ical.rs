use crate::task_engine::CalendarEventSelector;
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime,
    TimeZone, Utc, Weekday,
};
use chrono_tz::Tz as IanaTz;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

/// Normalizes a configured iCal URL string:
/// - Trims leading/trailing whitespace.
/// - Returns `None` if empty.
/// - Converts `webcal://` and `webcals://` (case-insensitive) to `https://`.
pub fn normalize_ical_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("webcals://") {
        let orig_rest = &trimmed[trimmed.len() - rest.len()..];
        Some(format!("https://{orig_rest}"))
    } else if let Some(rest) = lower.strip_prefix("webcal://") {
        let orig_rest = &trimmed[trimmed.len() - rest.len()..];
        Some(format!("https://{orig_rest}"))
    } else {
        Some(trimmed.to_string())
    }
}

/// Performs a live HTTP GET on `raw_url` and selects the matching event for `selector` at `now`.
pub fn fetch_and_select_event(
    raw_url: &str,
    selector: CalendarEventSelector,
    now: DateTime<Local>,
) -> Result<String, String> {
    let url = normalize_ical_url(raw_url)
        .ok_or_else(|| "No iCal feed URL configured. Run /config to set \"icalUrl\".".to_string())?;

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let response = agent
        .get(&url)
        .set("User-Agent", "TaskConveyor/0.1")
        .call()
        .map_err(|err| format!("Failed to fetch calendar feed: {err}"))?;

    let body = response
        .into_string()
        .map_err(|err| format!("Failed to read calendar feed response: {err}"))?;

    select_event_from_ical(&body, selector, now)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedEvent {
    pub summary: String,
    pub start: i64,
    pub end: i64,
}

impl ResolvedEvent {
    fn duration_secs(&self) -> i64 {
        (self.end - self.start).max(0)
    }
}

#[derive(Clone, Debug)]
struct ContentLine {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl ContentLine {
    fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug)]
enum ParsedDateTime {
    AllDay(NaiveDate),
    Utc(NaiveDateTime),
    Zoned { tzid: String, naive: NaiveDateTime },
    Floating(NaiveDateTime),
}

#[derive(Clone, Debug, Default)]
struct RawVEvent {
    uid: String,
    summary: String,
    status: Option<String>,
    dtstart: Option<ParsedDateTime>,
    dtend: Option<ParsedDateTime>,
    duration: Option<Duration>,
    rrule: Option<String>,
    exdates: Vec<ParsedDateTime>,
    recurrence_id: Option<ParsedDateTime>,
    attendee_partstats: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Debug)]
struct RRuleSpec {
    freq: Freq,
    interval: u32,
    count: Option<usize>,
    until: Option<ParsedDateTime>,
    byday: Vec<(Option<i32>, Weekday)>,
    bymonthday: Vec<i32>,
}

/// Parses `ics_text`, resolves single and recurring events against `now` (and the end of `now`'s
/// local day), and returns the selected event summary or a user-facing warning message.
pub fn select_event_from_ical<Tz: TimeZone>(
    ics_text: &str,
    selector: CalendarEventSelector,
    now: DateTime<Tz>,
) -> Result<String, String>
where
    Tz::Offset: Copy,
{
    let occurrences = resolve_occurrences_for_day(ics_text, &now)?;
    let now_ts = now.timestamp();

    let next_day_naive = now
        .date_naive()
        .succ_opt()
        .unwrap_or_else(|| now.date_naive())
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end_of_day_ts = resolve_floating_to_ts(&next_day_naive, &now.timezone());

    // Current events: start <= now < end.
    // Tie-break: most recent start time, then shortest duration.
    let current_event = occurrences
        .iter()
        .filter(|ev| ev.start <= now_ts && now_ts < ev.end)
        .max_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| b.duration_secs().cmp(&a.duration_secs()))
        });

    // Next events today: now < start < end_of_day.
    // Tie-break: earliest start time, then shortest duration.
    let next_event = occurrences
        .iter()
        .filter(|ev| now_ts < ev.start && ev.start < end_of_day_ts)
        .min_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| a.duration_secs().cmp(&b.duration_secs()))
        });

    match selector {
        CalendarEventSelector::CurrentOnly => current_event
            .map(|ev| ev.summary.clone())
            .ok_or_else(|| "No active calendar event right now.".to_string()),
        CalendarEventSelector::NextOnly => next_event
            .map(|ev| ev.summary.clone())
            .ok_or_else(|| "No upcoming calendar events today.".to_string()),
        CalendarEventSelector::CurrentOrNext => current_event
            .or(next_event)
            .map(|ev| ev.summary.clone())
            .ok_or_else(|| "No active or upcoming calendar events today.".to_string()),
    }
}

fn unfold_ical_lines(ics_text: &str) -> Vec<String> {
    let mut unfolded: Vec<String> = Vec::new();
    for raw_line in ics_text.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(last) = unfolded.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        if !line.is_empty() {
            unfolded.push(line.to_string());
        }
    }
    unfolded
}

fn parse_content_line(line: &str) -> Option<ContentLine> {
    let mut in_quotes = false;
    let mut colon_idx = None;
    for (idx, ch) in line.char_indices() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == ':' && !in_quotes {
            colon_idx = Some(idx);
            break;
        }
    }

    let colon_idx = colon_idx?;
    let left = &line[..colon_idx];
    let value = line[colon_idx + 1..].to_string();

    // Split `left` by `;` outside quotes
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_q = false;
    for (idx, ch) in left.char_indices() {
        if ch == '"' {
            in_q = !in_q;
        } else if ch == ';' && !in_q {
            parts.push(&left[start..idx]);
            start = idx + 1;
        }
    }
    parts.push(&left[start..]);

    let name = parts.first()?.trim().to_ascii_uppercase();
    let mut params = Vec::new();
    for param_str in parts.iter().skip(1) {
        if let Some((k, v)) = param_str.split_once('=') {
            let key = k.trim().to_ascii_uppercase();
            let val = v.trim().trim_matches('"').to_string();
            params.push((key, val));
        }
    }

    Some(ContentLine {
        name,
        params,
        value,
    })
}

pub fn unescape_ical_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push(' '),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn parse_ical_offset(raw: &str) -> Option<FixedOffset> {
    let s = raw.trim();
    if s.len() < 5 {
        return None;
    }
    let sign = match &s[0..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let hours: i32 = s[1..3].parse().ok()?;
    let mins: i32 = s[3..5].parse().ok()?;
    let secs: i32 = if s.len() >= 7 {
        s[5..7].parse().unwrap_or(0)
    } else {
        0
    };
    let total_secs = sign * (hours * 3600 + mins * 60 + secs);
    FixedOffset::east_opt(total_secs)
}

fn parse_datetime_prop(cl: &ContentLine) -> Option<ParsedDateTime> {
    let val = cl.value.trim();
    // If multi-value (e.g. EXDATE:20261118T100000Z,20261119T100000Z), parse the first one here;
    // multi-value EXDATEs are handled by `parse_datetime_list_prop`.
    let first_val = val.split(',').next()?.trim();
    parse_single_datetime_value(first_val, cl.param("VALUE"), cl.param("TZID"))
}

fn parse_datetime_list_prop(cl: &ContentLine) -> Vec<ParsedDateTime> {
    let value_param = cl.param("VALUE");
    let tzid_param = cl.param("TZID");
    cl.value
        .split(',')
        .filter_map(|part| parse_single_datetime_value(part.trim(), value_param, tzid_param))
        .collect()
}

fn parse_single_datetime_value(
    val: &str,
    value_param: Option<&str>,
    tzid_param: Option<&str>,
) -> Option<ParsedDateTime> {
    if value_param.is_some_and(|v| v.eq_ignore_ascii_case("DATE")) || !val.contains('T') {
        if val.len() >= 8 {
            let date = NaiveDate::parse_from_str(&val[..8], "%Y%m%d").ok()?;
            return Some(ParsedDateTime::AllDay(date));
        }
        return None;
    }

    let is_utc = val.ends_with('Z') || val.ends_with('z');
    let clean = val.trim_end_matches(['Z', 'z']);
    let naive = if clean.len() == 15 {
        NaiveDateTime::parse_from_str(clean, "%Y%m%dT%H%M%S").ok()?
    } else if clean.len() == 13 {
        NaiveDateTime::parse_from_str(clean, "%Y%m%dT%H%M").ok()?
    } else {
        return None;
    };

    if is_utc {
        Some(ParsedDateTime::Utc(naive))
    } else if let Some(tzid) = tzid_param {
        Some(ParsedDateTime::Zoned {
            tzid: tzid.to_string(),
            naive,
        })
    } else {
        Some(ParsedDateTime::Floating(naive))
    }
}

fn parse_ical_duration(raw: &str) -> Option<Duration> {
    let mut s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let negative = if let Some(rest) = s.strip_prefix('-') {
        s = rest;
        true
    } else {
        if let Some(rest) = s.strip_prefix('+') {
            s = rest;
        }
        false
    };

    s = s.strip_prefix('P')?;
    let mut total_secs: i64 = 0;
    let mut in_time = false;
    let mut num_buf = String::new();

    for ch in s.chars() {
        if ch == 'T' {
            in_time = true;
            continue;
        }
        if ch.is_ascii_digit() {
            num_buf.push(ch);
            continue;
        }
        let n: i64 = num_buf.parse().ok()?;
        num_buf.clear();
        match (in_time, ch) {
            (false, 'W') => total_secs += n * 7 * 86_400,
            (false, 'D') => total_secs += n * 86_400,
            (true, 'H') => total_secs += n * 3_600,
            (true, 'M') => total_secs += n * 60,
            (true, 'S') => total_secs += n,
            _ => return None,
        }
    }

    if negative {
        Some(Duration::seconds(-total_secs))
    } else {
        Some(Duration::seconds(total_secs))
    }
}

fn map_windows_tzid_to_iana(tzid: &str) -> Option<&'static str> {
    match tzid.trim() {
        "New Zealand Standard Time" => Some("Pacific/Auckland"),
        "AUS Eastern Standard Time" => Some("Australia/Sydney"),
        "E. Australia Standard Time" => Some("Australia/Brisbane"),
        "Cen. Australia Standard Time" => Some("Australia/Adelaide"),
        "W. Australia Standard Time" => Some("Australia/Perth"),
        "Pacific Standard Time" => Some("America/Los_Angeles"),
        "Mountain Standard Time" => Some("America/Denver"),
        "US Mountain Standard Time" => Some("America/Phoenix"),
        "Central Standard Time" => Some("America/Chicago"),
        "Eastern Standard Time" => Some("America/New_York"),
        "Atlantic Standard Time" => Some("America/Halifax"),
        "GMT Standard Time" => Some("Europe/London"),
        "W. Europe Standard Time" => Some("Europe/Berlin"),
        "Romance Standard Time" => Some("Europe/Paris"),
        "Central European Standard Time" => Some("Europe/Warsaw"),
        "E. Europe Standard Time" => Some("Europe/Bucharest"),
        "FLE Standard Time" => Some("Europe/Helsinki"),
        "India Standard Time" => Some("Asia/Kolkata"),
        "China Standard Time" => Some("Asia/Shanghai"),
        "Singapore Standard Time" => Some("Asia/Singapore"),
        "Tokyo Standard Time" => Some("Asia/Tokyo"),
        "Korea Standard Time" => Some("Asia/Seoul"),
        "UTC" | "Etc/UTC" | "Etc/GMT" => Some("UTC"),
        _ => None,
    }
}

fn resolve_floating_to_ts<Tz: TimeZone>(naive: &NaiveDateTime, local_tz: &Tz) -> i64 {
    local_tz
        .from_local_datetime(naive)
        .earliest()
        .or_else(|| local_tz.from_local_datetime(naive).latest())
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| Utc.from_utc_datetime(naive).timestamp())
}

fn resolve_zoned_to_ts<Tz: TimeZone>(
    tzid: &str,
    naive: &NaiveDateTime,
    vtimezones: &HashMap<String, FixedOffset>,
    local_tz: &Tz,
) -> i64 {
    let iana_candidate = map_windows_tzid_to_iana(tzid).unwrap_or(tzid);
    if let Ok(tz) = IanaTz::from_str(iana_candidate) {
        if let Some(dt) = tz
            .from_local_datetime(naive)
            .earliest()
            .or_else(|| tz.from_local_datetime(naive).latest())
        {
            return dt.timestamp();
        }
    }

    if let Some(offset) = vtimezones.get(tzid) {
        if let Some(dt) = offset.from_local_datetime(naive).single() {
            return dt.timestamp();
        }
    }

    resolve_floating_to_ts(naive, local_tz)
}

fn resolve_parsed_datetime_to_ts<Tz: TimeZone>(
    dt: &ParsedDateTime,
    vtimezones: &HashMap<String, FixedOffset>,
    local_tz: &Tz,
) -> Option<i64> {
    match dt {
        ParsedDateTime::AllDay(_) => None,
        ParsedDateTime::Utc(naive) => Some(Utc.from_utc_datetime(naive).timestamp()),
        ParsedDateTime::Zoned { tzid, naive } => {
            Some(resolve_zoned_to_ts(tzid, naive, vtimezones, local_tz))
        }
        ParsedDateTime::Floating(naive) => Some(resolve_floating_to_ts(naive, local_tz)),
    }
}

fn is_event_declined(ev: &RawVEvent, calendar_owner: Option<&str>) -> bool {
    if ev.attendee_partstats.is_empty() {
        return false;
    }

    if let Some(owner) = calendar_owner {
        let owner_clean = owner
            .trim()
            .strip_prefix("mailto:")
            .unwrap_or(owner.trim())
            .to_ascii_lowercase();
        if !owner_clean.is_empty() {
            for (attendee, partstat) in &ev.attendee_partstats {
                let att_clean = attendee
                    .trim()
                    .strip_prefix("mailto:")
                    .unwrap_or(attendee.trim())
                    .to_ascii_lowercase();
                if att_clean == owner_clean {
                    return partstat.eq_ignore_ascii_case("DECLINED");
                }
            }
        }
    }

    // If no owner email matched, treat the event as declined when there is at least one
    // `PARTSTAT=DECLINED` and no attendee with `ACCEPTED` or `TENTATIVE` (e.g. single-user invite).
    let any_declined = ev
        .attendee_partstats
        .iter()
        .any(|(_, p)| p.eq_ignore_ascii_case("DECLINED"));
    let any_accepted_or_tentative = ev.attendee_partstats.iter().any(|(_, p)| {
        p.eq_ignore_ascii_case("ACCEPTED") || p.eq_ignore_ascii_case("TENTATIVE")
    });

    any_declined && !any_accepted_or_tentative
}

fn parse_weekday(s: &str) -> Option<(Option<i32>, Weekday)> {
    let trimmed = s.trim();
    if trimmed.len() < 2 {
        return None;
    }
    let (ord_str, day_str) = trimmed.split_at(trimmed.len() - 2);
    let weekday = match day_str.to_ascii_uppercase().as_str() {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    };
    let ord = if ord_str.is_empty() {
        None
    } else {
        Some(ord_str.parse::<i32>().ok()?)
    };
    Some((ord, weekday))
}

fn parse_rrule(raw: &str) -> Option<RRuleSpec> {
    let mut freq = None;
    let mut interval = 1u32;
    let mut count = None;
    let mut until = None;
    let mut byday = Vec::new();
    let mut bymonthday = Vec::new();

    for part in raw.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        match k.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                freq = match v.trim().to_ascii_uppercase().as_str() {
                    "DAILY" => Some(Freq::Daily),
                    "WEEKLY" => Some(Freq::Weekly),
                    "MONTHLY" => Some(Freq::Monthly),
                    "YEARLY" => Some(Freq::Yearly),
                    _ => None,
                };
            }
            "INTERVAL" => {
                if let Ok(parsed) = v.trim().parse::<u32>()
                    && parsed > 0
                {
                    interval = parsed;
                }
            }
            "COUNT" => {
                count = v.trim().parse::<usize>().ok();
            }
            "UNTIL" => {
                until = parse_single_datetime_value(v.trim(), None, None);
            }
            "BYDAY" => {
                for token in v.split(',') {
                    if let Some(wd) = parse_weekday(token) {
                        byday.push(wd);
                    }
                }
            }
            "BYMONTHDAY" => {
                for token in v.split(',') {
                    if let Ok(md) = token.trim().parse::<i32>() {
                        bymonthday.push(md);
                    }
                }
            }
            _ => {}
        }
    }

    Some(RRuleSpec {
        freq: freq?,
        interval,
        count,
        until,
        byday,
        bymonthday,
    })
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(30)
}

fn matches_nth_weekday_in_month(date: NaiveDate, ord: Option<i32>, target_wd: Weekday) -> bool {
    if date.weekday() != target_wd {
        return false;
    }
    match ord {
        None => true,
        Some(n) if n > 0 => {
            let occurrence_idx = ((date.day() - 1) / 7) as i32 + 1;
            occurrence_idx == n
        }
        Some(n) if n < 0 => {
            let total_days = days_in_month(date.year(), date.month());
            let from_end = ((total_days - date.day()) / 7) as i32 + 1;
            from_end == -n
        }
        _ => false,
    }
}

fn expand_rrule_occurrences<Tz: TimeZone>(
    ev: &RawVEvent,
    spec: &RRuleSpec,
    duration_secs: i64,
    exdate_timestamps: &HashSet<i64>,
    exdate_dates: &HashSet<NaiveDate>,
    vtimezones: &HashMap<String, FixedOffset>,
    local_tz: &Tz,
    window_start_ts: i64,
    window_end_ts: i64,
) -> Vec<ResolvedEvent> {
    let Some(dtstart) = &ev.dtstart else {
        return Vec::new();
    };

    let (start_naive, tzid_opt, is_utc) = match dtstart {
        ParsedDateTime::AllDay(_) => return Vec::new(),
        ParsedDateTime::Utc(n) => (*n, None, true),
        ParsedDateTime::Zoned { tzid, naive } => (*naive, Some(tzid.as_str()), false),
        ParsedDateTime::Floating(n) => (*n, None, false),
    };

    let resolve_naive = |n: NaiveDateTime| -> i64 {
        if is_utc {
            Utc.from_utc_datetime(&n).timestamp()
        } else if let Some(tzid) = tzid_opt {
            resolve_zoned_to_ts(tzid, &n, vtimezones, local_tz)
        } else {
            resolve_floating_to_ts(&n, local_tz)
        }
    };

    let until_ts: Option<i64> = spec.until.as_ref().map(|u| match u {
        ParsedDateTime::AllDay(d) => {
            let end_naive = d
                .and_hms_opt(23, 59, 59)
                .unwrap_or_else(|| d.and_time(NaiveTime::default()));
            resolve_naive(end_naive)
        }
        other => resolve_parsed_datetime_to_ts(other, vtimezones, local_tz).unwrap_or(i64::MAX),
    });

    let start_date = start_naive.date();
    let start_time = start_naive.time();

    let mut results = Vec::new();
    let mut emitted_count = 0usize;
    let max_days = 3650i64; // Up to 10 years of recurrence expansion

    for day_offset in 0..=max_days {
        let Some(candidate_date) = start_date.checked_add_signed(Duration::days(day_offset)) else {
            break;
        };
        let candidate_naive = candidate_date.and_time(start_time);
        let occ_start_ts = resolve_naive(candidate_naive);

        if occ_start_ts > window_end_ts {
            break;
        }
        if let Some(u_ts) = until_ts
            && occ_start_ts > u_ts
        {
            break;
        }

        let matches_pattern = match spec.freq {
            Freq::Daily => {
                let interval = i64::from(spec.interval);
                day_offset % interval == 0
                    && (spec.byday.is_empty()
                        || spec
                            .byday
                            .iter()
                            .any(|(_, wd)| *wd == candidate_date.weekday()))
            }
            Freq::Weekly => {
                // Compute weeks elapsed aligned to the week start (Monday)
                let start_week_monday = start_date.week(Weekday::Mon).first_day();
                let cand_week_monday = candidate_date.week(Weekday::Mon).first_day();
                let weeks_diff = (cand_week_monday - start_week_monday).num_days() / 7;
                let interval = i64::from(spec.interval);
                if weeks_diff < 0 || weeks_diff % interval != 0 {
                    false
                } else if spec.byday.is_empty() {
                    candidate_date.weekday() == start_date.weekday()
                } else {
                    spec.byday
                        .iter()
                        .any(|(_, wd)| *wd == candidate_date.weekday())
                }
            }
            Freq::Monthly => {
                let months_diff = (candidate_date.year() - start_date.year()) * 12
                    + (candidate_date.month() as i32 - start_date.month() as i32);
                let interval = spec.interval as i32;
                if months_diff < 0 || months_diff % interval != 0 {
                    false
                } else if !spec.bymonthday.is_empty() {
                    let total_days = days_in_month(candidate_date.year(), candidate_date.month()) as i32;
                    spec.bymonthday.iter().any(|&md| {
                        if md > 0 {
                            candidate_date.day() as i32 == md
                        } else if md < 0 {
                            candidate_date.day() as i32 == (total_days + 1 + md)
                        } else {
                            false
                        }
                    })
                } else if !spec.byday.is_empty() {
                    spec.byday
                        .iter()
                        .any(|&(ord, wd)| matches_nth_weekday_in_month(candidate_date, ord, wd))
                } else {
                    candidate_date.day() == start_date.day()
                }
            }
            Freq::Yearly => {
                let years_diff = candidate_date.year() - start_date.year();
                let interval = spec.interval as i32;
                years_diff >= 0
                    && years_diff % interval == 0
                    && candidate_date.month() == start_date.month()
                    && candidate_date.day() == start_date.day()
            }
        };

        if !matches_pattern {
            continue;
        }

        emitted_count += 1;
        if let Some(max_count) = spec.count
            && emitted_count > max_count
        {
            break;
        }

        let occ_end_ts = occ_start_ts + duration_secs;
        if occ_end_ts <= window_start_ts {
            continue;
        }

        if exdate_timestamps.contains(&occ_start_ts) || exdate_dates.contains(&candidate_date) {
            continue;
        }

        results.push(ResolvedEvent {
            summary: ev.summary.clone(),
            start: occ_start_ts,
            end: occ_end_ts,
        });
    }

    results
}

fn resolve_occurrences_for_day<Tz: TimeZone>(
    ics_text: &str,
    now: &DateTime<Tz>,
) -> Result<Vec<ResolvedEvent>, String>
where
    Tz::Offset: Copy,
{
    let lines = unfold_ical_lines(ics_text);
    if !lines
        .iter()
        .any(|l| l.trim().eq_ignore_ascii_case("BEGIN:VCALENDAR"))
    {
        return Err("Invalid iCal feed (missing BEGIN:VCALENDAR).".to_string());
    }

    let local_tz = now.timezone();
    let mut vtimezones: HashMap<String, FixedOffset> = HashMap::new();
    let mut raw_events: Vec<RawVEvent> = Vec::new();
    let mut calendar_owner: Option<String> = None;

    let mut in_vtimezone = false;
    let mut current_tzid: Option<String> = None;
    let mut in_vevent = false;
    let mut current_event = RawVEvent::default();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VTIMEZONE") {
            in_vtimezone = true;
            current_tzid = None;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:VTIMEZONE") {
            in_vtimezone = false;
            current_tzid = None;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("BEGIN:VEVENT") {
            in_vevent = true;
            current_event = RawVEvent::default();
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:VEVENT") {
            if in_vevent {
                raw_events.push(current_event);
                current_event = RawVEvent::default();
                in_vevent = false;
            }
            continue;
        }

        let Some(cl) = parse_content_line(trimmed) else {
            continue;
        };

        if in_vtimezone {
            if cl.name == "TZID" {
                current_tzid = Some(cl.value.trim().to_string());
            } else if cl.name == "TZOFFSETTO"
                && let Some(ref tzid) = current_tzid
                && let Some(offset) = parse_ical_offset(&cl.value)
            {
                vtimezones.entry(tzid.clone()).or_insert(offset);
            }
            continue;
        }

        if !in_vevent {
            if cl.name == "X-WR-CALNAME" && cl.value.contains('@') {
                calendar_owner = Some(cl.value.trim().to_string());
            }
            continue;
        }

        match cl.name.as_str() {
            "UID" => current_event.uid = cl.value.trim().to_string(),
            "SUMMARY" => current_event.summary = unescape_ical_text(&cl.value),
            "STATUS" => current_event.status = Some(cl.value.trim().to_ascii_uppercase()),
            "DTSTART" => current_event.dtstart = parse_datetime_prop(&cl),
            "DTEND" => current_event.dtend = parse_datetime_prop(&cl),
            "DURATION" => current_event.duration = parse_ical_duration(&cl.value),
            "RRULE" => current_event.rrule = Some(cl.value.trim().to_string()),
            "EXDATE" => current_event.exdates.extend(parse_datetime_list_prop(&cl)),
            "RECURRENCE-ID" => current_event.recurrence_id = parse_datetime_prop(&cl),
            "ATTENDEE" => {
                if let Some(partstat) = cl.param("PARTSTAT") {
                    current_event
                        .attendee_partstats
                        .push((cl.value.clone(), partstat.to_string()));
                }
            }
            _ => {}
        }
    }

    // Collect `RECURRENCE-ID` overrides per UID so master recurring events skip overridden instances
    let mut overridden_by_uid: HashMap<String, HashSet<i64>> = HashMap::new();
    for ev in &raw_events {
        if !ev.uid.is_empty()
            && let Some(ref rec_id) = ev.recurrence_id
            && let Some(ts) = resolve_parsed_datetime_to_ts(rec_id, &vtimezones, &local_tz)
        {
            overridden_by_uid
                .entry(ev.uid.clone())
                .or_default()
                .insert(ts);
        }
    }

    let day_start_naive = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let next_day_naive = now
        .date_naive()
        .succ_opt()
        .unwrap_or_else(|| now.date_naive())
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let window_start_ts = resolve_floating_to_ts(&day_start_naive, &local_tz);
    let window_end_ts = resolve_floating_to_ts(&next_day_naive, &local_tz);

    let mut occurrences = Vec::new();

    for ev in &raw_events {
        if ev.summary.is_empty() {
            continue;
        }
        if ev.status.as_deref() == Some("CANCELLED") {
            continue;
        }
        if is_event_declined(ev, calendar_owner.as_deref()) {
            continue;
        }

        let Some(ref dtstart) = ev.dtstart else {
            continue;
        };
        let Some(start_ts) = resolve_parsed_datetime_to_ts(dtstart, &vtimezones, &local_tz) else {
            // All-day event (`ParsedDateTime::AllDay`) -> ignored
            continue;
        };

        let end_ts = if let Some(ref dtend) = ev.dtend {
            match resolve_parsed_datetime_to_ts(dtend, &vtimezones, &local_tz) {
                Some(ts) => ts,
                None => continue, // All-day end date
            }
        } else if let Some(dur) = ev.duration {
            start_ts + dur.num_seconds()
        } else {
            start_ts + 3_600
        };

        let duration_secs = (end_ts - start_ts).max(0);

        if let Some(ref rrule_raw) = ev.rrule
            && ev.recurrence_id.is_none()
            && let Some(spec) = parse_rrule(rrule_raw)
        {
            let mut exdate_timestamps: HashSet<i64> = ev
                .exdates
                .iter()
                .filter_map(|ex| resolve_parsed_datetime_to_ts(ex, &vtimezones, &local_tz))
                .collect();
            let exdate_dates: HashSet<NaiveDate> = ev
                .exdates
                .iter()
                .filter_map(|ex| match ex {
                    ParsedDateTime::AllDay(d) => Some(*d),
                    _ => None,
                })
                .collect();

            if let Some(overrides) = overridden_by_uid.get(&ev.uid) {
                exdate_timestamps.extend(overrides.iter().copied());
            }

            occurrences.extend(expand_rrule_occurrences(
                ev,
                &spec,
                duration_secs,
                &exdate_timestamps,
                &exdate_dates,
                &vtimezones,
                &local_tz,
                window_start_ts,
                window_end_ts,
            ));
        } else if end_ts > window_start_ts && start_ts <= window_end_ts {
            occurrences.push(ResolvedEvent {
                summary: ev.summary.clone(),
                start: start_ts,
                end: end_ts,
            });
        }
    }

    Ok(occurrences)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_tz(hours: i32) -> FixedOffset {
        FixedOffset::east_opt(hours * 3600).unwrap()
    }

    #[test]
    fn test_normalize_ical_url() {
        assert_eq!(normalize_ical_url(""), None);
        assert_eq!(normalize_ical_url("   "), None);
        assert_eq!(
            normalize_ical_url("  https://example.com/cal.ics "),
            Some("https://example.com/cal.ics".to_string())
        );
        assert_eq!(
            normalize_ical_url("webcal://p01-calendars.icloud.com/published/2/abc"),
            Some("https://p01-calendars.icloud.com/published/2/abc".to_string())
        );
        assert_eq!(
            normalize_ical_url("WEBCALS://example.com/feed.ics"),
            Some("https://example.com/feed.ics".to_string())
        );
    }

    #[test]
    fn test_unfolding_and_unescaping() {
        let ics = "BEGIN:VCALENDAR\r\n\
BEGIN:VEVENT\r\n\
UID:1\r\n\
SUMMARY:Sprint\\, Plan\r\n \
 ning \\; Review\\nTeam\r\n\
DTSTART:20261118T100000\r\n\
DTEND:20261118T110000\r\n\
END:VEVENT\r\n\
END:VCALENDAR";

        let tz = fixed_tz(13);
        let now = tz.with_ymd_and_hms(2026, 11, 18, 10, 30, 0).unwrap();
        let summary =
            select_event_from_ical(ics, CalendarEventSelector::CurrentOnly, now).unwrap();
        assert_eq!(summary, "Sprint, Planning ; Review Team");
    }

    #[test]
    fn test_filters_all_day_cancelled_and_declined_events() {
        let ics = "BEGIN:VCALENDAR\n\
X-WR-CALNAME:alice@example.com\n\
BEGIN:VEVENT\n\
UID:all-day\n\
SUMMARY:Company Holiday\n\
DTSTART;VALUE=DATE:20261118\n\
DTEND;VALUE=DATE:20261119\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:cancelled\n\
SUMMARY:Cancelled Sync\n\
STATUS:CANCELLED\n\
DTSTART:20261118T100000\n\
DTEND:20261118T110000\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:declined\n\
SUMMARY:Declined Meeting\n\
ATTENDEE;PARTSTAT=ACCEPTED:mailto:bob@example.com\n\
ATTENDEE;PARTSTAT=DECLINED:mailto:alice@example.com\n\
DTSTART:20261118T100000\n\
DTEND:20261118T110000\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:upcoming\n\
SUMMARY:Afternoon Architecture Review\n\
ATTENDEE;PARTSTAT=DECLINED:mailto:coworker@example.com\n\
ATTENDEE;PARTSTAT=ACCEPTED:mailto:alice@example.com\n\
DTSTART:20261118T140000\n\
DTEND:20261118T150000\n\
END:VEVENT\n\
END:VCALENDAR";

        let tz = fixed_tz(0);
        let now = tz.with_ymd_and_hms(2026, 11, 18, 10, 15, 0).unwrap();

        // At 10:15, all-day, cancelled, and Alice-declined events are ignored -> no active event
        assert!(select_event_from_ical(ics, CalendarEventSelector::CurrentOnly, now).is_err());

        // Fallback (CurrentOrNext) and NextOnly pick the 14:00 meeting where Alice accepted
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::CurrentOrNext, now).unwrap(),
            "Afternoon Architecture Review"
        );
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::NextOnly, now).unwrap(),
            "Afternoon Architecture Review"
        );
    }

    #[test]
    fn test_overlapping_events_and_strict_boundaries() {
        let ics = "BEGIN:VCALENDAR\n\
BEGIN:VEVENT\n\
UID:block\n\
SUMMARY:Morning Focus Block\n\
DTSTART:20261118T090000\n\
DTEND:20261118T120000\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:standup\n\
SUMMARY:Team Standup\n\
DTSTART:20261118T100000\n\
DTEND:20261118T103000\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:quick-sync\n\
SUMMARY:Quick Triage\n\
DTSTART:20261118T100000\n\
DTEND:20261118T101500\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:next-up\n\
SUMMARY:1:1 with Manager\n\
DTSTART:20261118T103000\n\
DTEND:20261118T110000\n\
END:VEVENT\n\
END:VCALENDAR";

        let tz = fixed_tz(0);

        // At 10:05: Morning Focus Block (09:00), Team Standup (10:00-10:30), and Quick Triage (10:00-10:15)
        // all overlap. Quick Triage started most recently (10:00) and has the shortest duration (15m).
        let at_10_05 = tz.with_ymd_and_hms(2026, 11, 18, 10, 5, 0).unwrap();
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::CurrentOnly, at_10_05).unwrap(),
            "Quick Triage"
        );

        // At 10:20: Quick Triage has ended, Team Standup (10:00-10:30) is active over Morning Focus Block (09:00)
        let at_10_20 = tz.with_ymd_and_hms(2026, 11, 18, 10, 20, 0).unwrap();
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::CurrentOnly, at_10_20).unwrap(),
            "Team Standup"
        );
        // At 10:20, `/c next` picks 1:1 with Manager (10:30)
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::NextOnly, at_10_20).unwrap(),
            "1:1 with Manager"
        );

        // At exact boundary 10:30:00: Team Standup (`start <= now < end`) has ended; 1:1 with Manager is now current!
        let at_10_30 = tz.with_ymd_and_hms(2026, 11, 18, 10, 30, 0).unwrap();
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::CurrentOnly, at_10_30).unwrap(),
            "1:1 with Manager"
        );
    }

    #[test]
    fn test_next_event_restricted_to_same_local_day() {
        let ics = "BEGIN:VCALENDAR\n\
BEGIN:VEVENT\n\
UID:tomorrow\n\
SUMMARY:Tomorrow Morning Standup\n\
DTSTART:20261119T090000\n\
DTEND:20261119T093000\n\
END:VEVENT\n\
END:VCALENDAR";

        let tz = fixed_tz(0);
        let evening = tz.with_ymd_and_hms(2026, 11, 18, 18, 0, 0).unwrap();
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::CurrentOrNext, evening),
            Err("No active or upcoming calendar events today.".to_string())
        );
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::NextOnly, evening),
            Err("No upcoming calendar events today.".to_string())
        );
    }

    #[test]
    fn test_rrule_weekly_with_exdate_and_recurrence_id_override() {
        // 2026-11-18 is a Wednesday
        let ics = "BEGIN:VCALENDAR\n\
BEGIN:VEVENT\n\
UID:recurring-sync\n\
SUMMARY:Weekly Eng Sync\n\
DTSTART:20261104T110000\n\
DTEND:20261104T113000\n\
RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR\n\
EXDATE:20261118T110000\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:recurring-standup\n\
SUMMARY:Daily Standup\n\
DTSTART:20261101T093000\n\
DTEND:20261101T094500\n\
RRULE:FREQ=DAILY\n\
END:VEVENT\n\
BEGIN:VEVENT\n\
UID:recurring-standup\n\
RECURRENCE-ID:20261118T093000\n\
SUMMARY:Rescheduled Standup\n\
DTSTART:20261118T150000\n\
DTEND:20261118T153000\n\
END:VEVENT\n\
END:VCALENDAR";

        let tz = fixed_tz(0);
        // At 09:35 on 2026-11-18: Daily Standup was overridden by RECURRENCE-ID to 15:00,
        // and Weekly Eng Sync at 11:00 was excluded via EXDATE!
        let at_09_35 = tz.with_ymd_and_hms(2026, 11, 18, 9, 35, 0).unwrap();
        assert!(select_event_from_ical(ics, CalendarEventSelector::CurrentOnly, at_09_35).is_err());
        assert_eq!(
            select_event_from_ical(ics, CalendarEventSelector::CurrentOrNext, at_09_35).unwrap(),
            "Rescheduled Standup"
        );
    }
}
