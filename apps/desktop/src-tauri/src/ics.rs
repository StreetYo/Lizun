//! Reading iCalendar (.ics) feeds: the events that fall into a time window, with recurring
//! events expanded.
//!
//! Covers what calendar services put into their feeds: all-day and timed events, time zones by
//! IANA or Windows name, RRULE with EXDATE and RDATE, moved or cancelled occurrences
//! (RECURRENCE-ID) and cancelled events. Floating times and unknown zones are read as local time,
//! and VTIMEZONE definitions are not read: every zone a service uses also has a known name.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::str::FromStr;
use std::time::{Duration, Instant};

use chrono::{DateTime, Days, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc};
use rrule::{Frequency, RRule, RRuleSet, Tz, Unvalidated};
use serde::Serialize;

/// Most occurrences one recurring event produces in the window.
const MAX_OCCURRENCES: usize = 500;
/// Most events returned for the window.
const MAX_EVENTS: usize = 2_000;
/// Most VEVENTs read from one feed. Calendars with years of history have a few thousand.
const MAX_COMPONENTS: usize = 50_000;
/// Most occurrences one recurring event is stepped through, counting those before the window: a
/// weekly meeting since 2000 has about 1,400. Rules that repeat at a fixed step, such as every
/// day or every hour, skip ahead to the window instead.
const MAX_STEPS: usize = 100_000;
/// Most occurrences of all recurring events together stepped through for one feed; recurring
/// events left after that are not shown.
const MAX_TOTAL_STEPS: usize = 2_000_000;
/// The same for the time expanding the recurring events of one feed takes.
const MAX_EXPANSION_TIME: Duration = Duration::from_secs(5);
/// Most times of day a daily or longer rule lists with BYHOUR, BYMINUTE and BYSECOND together;
/// each of them is made for every day of a period at once.
const MAX_TIMES_OF_DAY: usize = 60;
/// Most dates one event lists in RDATE or in EXDATE.
const MAX_DATES: usize = 10_000;
/// Titles and places are cut to this many characters; the island shows one line of each.
const MAX_TEXT_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub title: String,
    pub location: String,
    /// Milliseconds since the Unix epoch. All-day events start and end at local midnight, and
    /// `end` is exclusive.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

/// Events that overlap `from..to`, sorted by start; all-day events come first on their day.
/// All-day events and times without a zone are placed in `local`, the user's time zone.
pub fn events_between(text: &str, from: DateTime<Utc>, to: DateTime<Utc>, local: Tz) -> Vec<Event> {
    let mut zones = Zones { local, by_name: HashMap::new() };
    let components = components(text, &mut zones);

    // Occurrences of recurring events that were moved or cancelled, by UID.
    let mut replaced: HashMap<&str, HashSet<i64>> = HashMap::new();
    for component in &components {
        if let Some(id) = component.recurrence_id {
            replaced.entry(&component.uid).or_default().insert(id.timestamp_millis());
        }
    }

    // Only the earliest MAX_EVENTS occurrences are kept while going through them, by their sort
    // key and the index of their component; the events are made from those at the end.
    let (from_ms, to_ms) = (from.timestamp_millis(), to.timestamp_millis());
    let mut earliest: BinaryHeap<(i64, bool, i64, usize)> = BinaryHeap::with_capacity(MAX_EVENTS + 1);
    let mut budget = Budget { steps: MAX_TOTAL_STEPS, deadline: Instant::now() + MAX_EXPANSION_TIME };
    for (index, component) in components.iter().enumerate().filter(|(_, c)| !c.cancelled) {
        let skip = component.recurrence_id.is_none().then(|| replaced.get(component.uid.as_str())).flatten();
        for start in component.starts(from, to, &mut budget) {
            let start_ms = start.timestamp_millis();
            if skip.is_some_and(|skip| skip.contains(&start_ms)) {
                continue;
            }
            let end_ms = component.end_of(start).timestamp_millis().max(start_ms);
            let overlaps = if end_ms > start_ms { end_ms > from_ms } else { start_ms >= from_ms };
            if overlaps && start_ms < to_ms {
                earliest.push((start_ms, !component.start.all_day, end_ms, index));
                if earliest.len() > MAX_EVENTS {
                    earliest.pop();
                }
            }
        }
    }
    earliest
        .into_sorted_vec()
        .into_iter()
        .map(|(start, timed, end, index)| Event {
            title: components[index].title.clone(),
            location: components[index].location.clone(),
            start,
            end,
            all_day: !timed,
        })
        .collect()
}

/// What is left of the work allowed for expanding the recurring events of one feed.
struct Budget {
    steps: usize,
    deadline: Instant,
}

impl Budget {
    /// False once the steps or the time have run out.
    fn left(&mut self) -> bool {
        if self.steps > 0 && Instant::now() >= self.deadline {
            self.steps = 0;
            Self::ran_out();
        }
        self.steps > 0
    }

    /// Takes one step; false once the steps or the time have run out.
    fn step(&mut self) -> bool {
        if self.steps.is_multiple_of(1024) && !self.left() {
            return false;
        }
        self.steps -= 1;
        if self.steps == 0 {
            Self::ran_out();
        }
        true
    }

    fn ran_out() {
        log::warn!("the recurring events of the calendar feed take too long to expand; the rest are left out");
    }
}

/// One content line, such as `DTSTART;TZID=Europe/Moscow:20261009T100000`.
#[derive(Debug)]
struct Property {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Property {
    fn param(&self, name: &str) -> Option<&str> {
        self.params.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    fn text(&self) -> String {
        let mut text = String::with_capacity(self.value.len());
        let mut chars = self.value.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('n' | 'N') => text.push(' '),
                    Some(escaped) => text.push(escaped),
                    None => {}
                },
                '\r' | '\n' => text.push(' '),
                c => text.push(c),
            }
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        match text.char_indices().nth(MAX_TEXT_CHARS) {
            Some((cut, _)) => format!("{}…", &text[..cut]),
            None => text,
        }
    }
}

/// The content lines of the file, with lines folded onto the next one joined.
fn unfold(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut lines = text.lines().peekable();
    std::iter::from_fn(move || {
        let mut line = lines.by_ref().find(|line| !line.is_empty())?.to_string();
        while let Some(next) = lines.peek() {
            if let Some(rest) = next.strip_prefix([' ', '\t']) {
                line.push_str(rest);
            } else if !next.is_empty() {
                break;
            }
            lines.next();
        }
        Some(line)
    })
}

fn parse_line(line: &str) -> Option<Property> {
    let mut parts = Vec::new();
    let mut part_start = 0;
    let mut quoted = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                parts.push(&line[part_start..i]);
                part_start = i + 1;
            }
            ':' if !quoted => {
                parts.push(&line[part_start..i]);
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let mut parts = parts.into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts
        .filter_map(|param| {
            let (key, value) = param.split_once('=')?;
            Some((key.trim().to_ascii_uppercase(), value.trim_matches('"').to_string()))
        })
        .collect();
    Some(Property { name, params, value: line[colon? + 1..].to_string() })
}

/// Every VEVENT that has a start, read from its own properties and not from those of
/// components nested in it, such as VALARM. Each one is read as its properties come, so those of
/// the whole file are never held at once.
fn components(text: &str, zones: &mut Zones) -> Vec<Component> {
    let mut events = Vec::new();
    let mut current: Option<Reader> = None;
    let mut nested = 0usize;
    for line in unfold(text) {
        let Some(property) = parse_line(&line) else { continue };
        match (property.name.as_str(), current.as_mut()) {
            ("BEGIN", None) if property.value.trim().eq_ignore_ascii_case("VEVENT") => {
                if events.len() == MAX_COMPONENTS {
                    log::warn!("the calendar feed has more than {MAX_COMPONENTS} events; the rest are left out");
                    break;
                }
                current = Some(Reader::default());
            }
            ("BEGIN", Some(_)) => nested += 1,
            ("END", Some(_)) if nested > 0 => nested -= 1,
            ("END", Some(_)) => events.extend(current.take().and_then(Reader::finish)),
            (_, Some(reader)) if nested == 0 => reader.read(&property, zones),
            _ => {}
        }
    }
    events
}

/// Time zones by the name a file uses, looked up once per file, and the user's own zone.
struct Zones {
    local: Tz,
    by_name: HashMap<String, Option<Tz>>,
}

impl Zones {
    fn get(&mut self, name: &str) -> Option<Tz> {
        *self.by_name.entry(name.to_string()).or_insert_with(|| zone_by_name(name))
    }
}

fn zone_by_name(name: &str) -> Option<Tz> {
    let name = name.trim();
    if let Ok(zone) = chrono_tz::Tz::from_str(name) {
        return Some(zone.into());
    }
    // Some exporters put a path in front of the IANA name, as in /mozilla.org/20050126_1/Europe/Berlin.
    let mut rest = name;
    while let Some((_, tail)) = rest.split_once('/') {
        if let Ok(zone) = chrono_tz::Tz::from_str(tail) {
            return Some(zone.into());
        }
        rest = tail;
    }
    // Outlook and Exchange use Windows names, such as "Russian Standard Time".
    let iana = crate::platform::iana_time_zone(name)?;
    chrono_tz::Tz::from_str(&iana).ok().map(Tz::from)
}

#[derive(Debug, Clone, Copy)]
struct Time {
    at: DateTime<Tz>,
    all_day: bool,
}

fn parse_time(property: &Property, zones: &mut Zones) -> Option<Time> {
    parse_times(property, zones).into_iter().next()
}

/// Every value of a date or date-time property; EXDATE and RDATE can list several.
fn parse_times(property: &Property, zones: &mut Zones) -> Vec<Time> {
    let all_day = property.param("VALUE").is_some_and(|value| value.eq_ignore_ascii_case("DATE"));
    let zone = property.param("TZID").and_then(|name| zones.get(name));
    property.value.split(',').filter_map(|value| parse_value(value, all_day, zone, zones.local)).collect()
}

/// A date is midnight in `local`; a time is in `zone`, in UTC with a `Z`, or else in `local`.
fn parse_value(value: &str, all_day: bool, zone: Option<Tz>, local: Tz) -> Option<Time> {
    let value = value.trim();
    if all_day || value.len() == 8 {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
        return Some(Time { at: in_zone(local, date.and_time(NaiveTime::MIN))?, all_day: true });
    }
    let (value, zone) = match value.strip_suffix(['Z', 'z']) {
        Some(value) => (value, Tz::UTC),
        None => (value, zone.unwrap_or(local)),
    };
    let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    Some(Time { at: in_zone(zone, naive)?, all_day: false })
}

/// A time skipped by a daylight saving change is moved an hour later.
fn in_zone(zone: Tz, naive: NaiveDateTime) -> Option<DateTime<Tz>> {
    zone.from_local_datetime(&naive)
        .earliest()
        .or_else(|| zone.from_local_datetime(&(naive + TimeDelta::hours(1))).earliest())
}

/// `P1D`, `PT1H30M`, `-P2W` and so on.
fn parse_duration(value: &str) -> Option<TimeDelta> {
    let value = value.trim();
    let (negative, value) = match value.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let mut total = TimeDelta::zero();
    let mut number = String::new();
    let mut in_time = false;
    for c in value.strip_prefix(['P', 'p'])?.chars() {
        match c.to_ascii_uppercase() {
            '0'..='9' => number.push(c),
            'T' => in_time = true,
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                let part = match (unit, in_time) {
                    ('W', false) => TimeDelta::try_weeks(n),
                    ('D', false) => TimeDelta::try_days(n),
                    ('H', true) => TimeDelta::try_hours(n),
                    ('M', true) => TimeDelta::try_minutes(n),
                    ('S', true) => TimeDelta::try_seconds(n),
                    _ => None,
                }?;
                total = total.checked_add(&part)?;
            }
        }
    }
    number.is_empty().then_some(if negative { -total } else { total })
}

#[derive(Debug, Clone, Copy)]
enum Length {
    /// All-day events last whole local days, whatever daylight saving does to their hours.
    Days(u64),
    Exact(TimeDelta),
}

struct Component {
    uid: String,
    title: String,
    location: String,
    start: Time,
    length: Length,
    rule: Option<String>,
    rdates: Vec<DateTime<Tz>>,
    exdates: Vec<DateTime<Tz>>,
    /// Set on an event that replaces one occurrence of a recurring event.
    recurrence_id: Option<DateTime<Tz>>,
    cancelled: bool,
}

/// Gathers the properties of one VEVENT as they come.
#[derive(Default)]
struct Reader {
    uid: String,
    title: String,
    location: String,
    start: Option<Time>,
    end: Option<Time>,
    duration: Option<TimeDelta>,
    rule: Option<String>,
    rdates: Vec<DateTime<Tz>>,
    exdates: Vec<DateTime<Tz>>,
    recurrence_id: Option<DateTime<Tz>>,
    cancelled: bool,
}

impl Reader {
    fn read(&mut self, property: &Property, zones: &mut Zones) {
        let add = |dates: &mut Vec<DateTime<Tz>>, zones: &mut Zones| {
            let room = MAX_DATES.saturating_sub(dates.len());
            dates.extend(parse_times(property, zones).iter().take(room).map(|time| time.at));
        };
        match property.name.as_str() {
            "UID" => self.uid = property.value.trim().to_string(),
            "SUMMARY" => self.title = property.text(),
            "LOCATION" => self.location = property.text(),
            "DTSTART" => self.start = parse_time(property, zones),
            "DTEND" => self.end = parse_time(property, zones),
            "DURATION" => self.duration = parse_duration(&property.value),
            "RRULE" if self.rule.is_none() => self.rule = Some(property.value.trim().to_string()),
            "RDATE" => add(&mut self.rdates, zones),
            "EXDATE" => add(&mut self.exdates, zones),
            "RECURRENCE-ID" => self.recurrence_id = parse_time(property, zones).map(|time| time.at),
            "STATUS" => self.cancelled = property.value.trim().eq_ignore_ascii_case("CANCELLED"),
            _ => {}
        }
    }

    fn finish(self) -> Option<Component> {
        let start = self.start?;
        let all_day = start.all_day;
        let length = match (self.end, self.duration) {
            (Some(end), _) if all_day && end.all_day => {
                let days = (end.at.date_naive() - start.at.date_naive()).num_days();
                Length::Days(days.max(1) as u64)
            }
            (Some(end), _) => Length::Exact(end.at - start.at),
            (None, Some(duration)) if all_day && duration.num_seconds() % 86_400 == 0 => {
                Length::Days(duration.num_days().max(1) as u64)
            }
            (None, Some(duration)) => Length::Exact(duration),
            (None, None) if all_day => Length::Days(1),
            (None, None) => Length::Exact(TimeDelta::zero()),
        };
        Some(Component {
            uid: self.uid,
            title: self.title,
            location: self.location,
            start,
            length,
            rule: self.rule,
            rdates: self.rdates,
            exdates: self.exdates,
            recurrence_id: self.recurrence_id,
            cancelled: self.cancelled,
        })
    }
}

impl Component {
    fn end_of(&self, start: DateTime<Tz>) -> DateTime<Tz> {
        match self.length {
            // All-day starts are midnights in the user's zone.
            Length::Days(days) => start
                .date_naive()
                .checked_add_days(Days::new(days))
                .and_then(|date| in_zone(start.timezone(), date.and_time(NaiveTime::MIN)))
                .unwrap_or(start),
            Length::Exact(length) => start.checked_add_signed(length).unwrap_or(start),
        }
    }

    /// Start times of the occurrences that can overlap `from..to`. Stepping through the
    /// occurrences of a recurring event takes from `budget`.
    fn starts(&self, from: DateTime<Utc>, to: DateTime<Utc>, budget: &mut Budget) -> Vec<DateTime<Tz>> {
        // An occurrence that starts before the window can last into it.
        let longest = match self.length {
            Length::Days(days) => TimeDelta::try_days(days as i64 + 1),
            Length::Exact(length) => Some(length.max(TimeDelta::zero())),
        };
        let after = longest.and_then(|longest| from.checked_sub_signed(longest)).unwrap_or(from);
        let within = |start: &DateTime<Tz>| *start >= after && *start < to;

        let Some(rule) = &self.rule else {
            let mut starts: Vec<_> = std::iter::once(self.start.at).chain(self.rdates.iter().copied()).collect();
            starts.retain(within);
            return starts;
        };
        // A rule that never matches can look for its next occurrence for a long time.
        if !budget.left() {
            return Vec::new();
        }
        match self.rule_set(rule, after) {
            Ok(set) => {
                // The occurrences come in order, from the first one on.
                let mut starts = Vec::new();
                let set = set.limit();
                for (steps, start) in set.into_iter().enumerate() {
                    if start >= to || starts.len() == MAX_OCCURRENCES || !budget.step() {
                        break;
                    }
                    if steps == MAX_STEPS {
                        log::warn!(
                            "a recurring event in the calendar feed repeats too often; only its first {MAX_STEPS} occurrences are read"
                        );
                        break;
                    }
                    if within(&start) {
                        starts.push(start);
                    }
                }
                starts
            }
            Err(err) => {
                log::debug!("reading {:?} as a single event, its RRULE does not work: {err}", self.title);
                Some(self.start.at).into_iter().filter(within).collect()
            }
        }
    }

    /// The rule with its exceptions. Its start is moved ahead towards `after` when that changes
    /// no occurrence from `after` on.
    fn rule_set(&self, rule: &str, after: DateTime<Utc>) -> Result<RRuleSet, String> {
        // UNTIL is read here: feeds write it in local time or as a date more often than the rrule
        // crate accepts.
        let mut until = None;
        let parts: Vec<&str> = rule
            .split(';')
            .filter(|part| match part.split_once('=') {
                Some((key, value)) if key.trim().eq_ignore_ascii_case("UNTIL") => {
                    until = Some(value.trim());
                    false
                }
                _ => !part.trim().is_empty(),
            })
            .collect();
        let mut rrule: RRule<Unvalidated> = parts.join(";").parse().map_err(|err: rrule::RRuleError| err.to_string())?;
        check_work(&rrule)?;
        let mut skip_to = after;
        if let Some(until) = until {
            let until = self.until(until).ok_or_else(|| format!("unreadable UNTIL {until:?}"))?;
            rrule = rrule.until(until.with_timezone(&Tz::UTC));
            skip_to = skip_to.min(until.with_timezone(&Utc));
        }
        let start = skip_ahead(&rrule, self.start.at, skip_to);
        let set = rrule.build(start).map_err(|err| err.to_string())?;
        Ok(set.set_rdates(self.rdates.clone()).set_exdates(self.exdates.clone()))
    }

    /// The last moment an occurrence can start. A date includes that whole day.
    fn until(&self, value: &str) -> Option<DateTime<Tz>> {
        let zone = self.start.at.timezone();
        if value.len() == 8 {
            let date = NaiveDate::parse_from_str(value, "%Y%m%d").ok()?;
            return in_zone(zone, date.and_hms_opt(23, 59, 59)?);
        }
        parse_value(value, false, Some(zone), zone).map(|time| time.at)
    }
}

/// Refuses rules that make the rrule crate do a lot of work for each occurrence. No calendar
/// service writes them.
fn check_work(rrule: &RRule<Unvalidated>) -> Result<(), String> {
    let (hours, minutes, seconds) =
        (rrule.get_by_hour().len(), rrule.get_by_minute().len(), rrule.get_by_second().len());
    if rrule.get_freq() > Frequency::Daily && hours + minutes + seconds > 0 {
        // Finding the next matching time steps through every hour, minute or second.
        return Err("BYHOUR, BYMINUTE or BYSECOND in a rule repeating more often than daily".into());
    }
    if hours.max(1) * minutes.max(1) * seconds.max(1) > MAX_TIMES_OF_DAY {
        return Err("too many times of day".into());
    }
    Ok(())
}

/// A later start for a rule that repeats at a fixed step, with the same occurrences from `target`
/// on, so that expanding it does not walk through every occurrence since a start years before.
/// These are rules without COUNT, which counts from the first start, and without BY* parts:
/// their occurrences are the start plus whole multiples of the step, in local time.
fn skip_ahead(rrule: &RRule<Unvalidated>, start: DateTime<Tz>, target: DateTime<Utc>) -> DateTime<Tz> {
    let period = match rrule.get_freq() {
        Frequency::Secondly => 1,
        Frequency::Minutely => 60,
        Frequency::Hourly => 3_600,
        Frequency::Daily => 86_400,
        Frequency::Weekly => 7 * 86_400,
        Frequency::Monthly | Frequency::Yearly => return start,
    };
    let fixed = rrule.get_count().is_none()
        && rrule.get_interval() > 0
        && rrule.get_by_set_pos().is_empty()
        && rrule.get_by_month().is_empty()
        && rrule.get_by_month_day().is_empty()
        && rrule.get_by_year_day().is_empty()
        && rrule.get_by_week_no().is_empty()
        && rrule.get_by_weekday().is_empty()
        && rrule.get_by_hour().is_empty()
        && rrule.get_by_minute().is_empty()
        && rrule.get_by_second().is_empty();
    if !fixed || target <= start {
        return start;
    }
    let step = period * i64::from(rrule.get_interval());
    let zone = start.timezone();
    let mut steps = target.signed_duration_since(start).num_seconds() / step;
    for _ in 0..8 {
        if steps <= 0 {
            break;
        }
        let later = TimeDelta::try_seconds(steps * step)
            .and_then(|skip| start.naive_local().checked_add_signed(skip))
            .and_then(|naive| zone.from_local_datetime(&naive).earliest());
        match later {
            Some(later) if later <= target => return later,
            // A change of the UTC offset since the start can put that local time after `target`.
            Some(later) => steps -= later.signed_duration_since(target).num_seconds() / step + 1,
            // A local time skipped by a daylight saving change cannot be the start; a few hours
            // earlier is outside the change.
            None => steps -= 3 * 3_600 / step + 1,
        }
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn calendar(events: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:test\r\n{events}END:VCALENDAR\r\n")
    }

    fn starts(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .map(|event| DateTime::from_timestamp_millis(event.start).unwrap().to_rfc3339())
            .collect()
    }

    fn local_midnight(y: i32, m: u32, d: u32) -> i64 {
        Local.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap().timestamp_millis()
    }

    #[test]
    fn reads_timed_and_all_day_events() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Stand-up\\, team\r\nLOCATION:Room 1\r\n\
             DTSTART;TZID=Europe/Moscow:20261009T100000\r\nDTEND;TZID=Europe/Moscow:20261009T103000\r\n\
             BEGIN:VALARM\r\nACTION:DISPLAY\r\nSUMMARY:Alarm text\r\nEND:VALARM\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261009\r\nDTEND;VALUE=DATE:20261011\r\nEND:VEVENT\r\n",
        );
        let events = events_between(&text, utc("2026-10-08T00:00:00Z"), utc("2026-10-12T00:00:00Z"), Tz::LOCAL);
        assert_eq!(events.len(), 2);
        let holiday = events.iter().find(|e| e.all_day).unwrap();
        assert_eq!(holiday.title, "Holiday");
        assert_eq!((holiday.start, holiday.end), (local_midnight(2026, 10, 9), local_midnight(2026, 10, 11)));
        let standup = events.iter().find(|e| !e.all_day).unwrap();
        assert_eq!(standup.title, "Stand-up, team");
        assert_eq!(standup.location, "Room 1");
        assert_eq!(standup.start, utc("2026-10-09T07:00:00Z").timestamp_millis());
        assert_eq!(standup.end - standup.start, 30 * 60 * 1000);
    }

    #[test]
    fn joins_folded_lines_and_reads_quoted_parameters() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Planning for the\r\n  quarter\r\n\
             DTSTART;TZID=\"/mozilla.org/20050126_1/Europe/Berlin\":20261009T090000\r\nDURATION:PT1H30M\r\nEND:VEVENT\r\n",
        );
        let events = events_between(&text, utc("2026-10-09T00:00:00Z"), utc("2026-10-10T00:00:00Z"), Tz::LOCAL);
        assert_eq!(events[0].title, "Planning for the quarter");
        assert_eq!(starts(&events), ["2026-10-09T07:00:00+00:00"]);
        assert_eq!(events[0].end - events[0].start, 90 * 60 * 1000);
    }

    #[test]
    fn expands_recurring_events_with_exceptions() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:weekly\r\nSUMMARY:Sync\r\nDTSTART;TZID=Europe/Moscow:20260105T120000\r\n\
             DTEND;TZID=Europe/Moscow:20260105T130000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,TH;UNTIL=20261231T235959Z\r\n\
             EXDATE;TZID=Europe/Moscow:20261008T120000\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:weekly\r\nRECURRENCE-ID;TZID=Europe/Moscow:20261012T120000\r\nSUMMARY:Sync (moved)\r\n\
             DTSTART;TZID=Europe/Moscow:20261012T150000\r\nDTEND;TZID=Europe/Moscow:20261012T160000\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:weekly\r\nRECURRENCE-ID;TZID=Europe/Moscow:20261015T120000\r\nSTATUS:CANCELLED\r\n\
             DTSTART;TZID=Europe/Moscow:20261015T120000\r\nEND:VEVENT\r\n",
        );
        let events = events_between(&text, utc("2026-10-05T00:00:00Z"), utc("2026-10-20T00:00:00Z"), Tz::LOCAL);
        let found: Vec<_> = events.iter().map(|e| e.title.as_str()).zip(starts(&events)).collect();
        assert_eq!(
            found,
            [
                ("Sync", "2026-10-05T09:00:00+00:00".to_string()),
                ("Sync (moved)", "2026-10-12T12:00:00+00:00".to_string()),
                ("Sync", "2026-10-19T09:00:00+00:00".to_string()),
            ]
        );
    }

    #[test]
    fn reads_until_in_local_time_and_as_a_date() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Daily\r\nDTSTART;TZID=Europe/Moscow:20261001T090000\r\n\
             RRULE:FREQ=DAILY;UNTIL=20261003T090000\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Daily all day\r\nDTSTART;VALUE=DATE:20261001\r\n\
             RRULE:FREQ=DAILY;UNTIL=20261002\r\nEND:VEVENT\r\n",
        );
        let events = events_between(&text, utc("2026-09-30T00:00:00Z"), utc("2026-10-10T00:00:00Z"), Tz::LOCAL);
        assert_eq!(events.iter().filter(|e| e.title == "Daily").count(), 3);
        assert_eq!(events.iter().filter(|e| e.title == "Daily all day").count(), 2);
    }

    #[test]
    fn keeps_events_that_started_before_the_window() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Trip\r\nDTSTART;VALUE=DATE:20261001\r\nDTEND;VALUE=DATE:20261010\r\n\
             RRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Over\r\nDTSTART:20261001T100000Z\r\nDTEND:20261001T110000Z\r\nEND:VEVENT\r\n",
        );
        let events = events_between(&text, utc("2026-10-08T00:00:00Z"), utc("2026-10-09T00:00:00Z"), Tz::LOCAL);
        assert_eq!(events.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Trip"]);
    }

    #[test]
    fn skips_cancelled_and_broken_events() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Cancelled\r\nSTATUS:CANCELLED\r\nDTSTART:20261009T100000Z\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:No start\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:c\r\nSUMMARY:Bad date\r\nDTSTART:2026-10-09\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:d\r\nSUMMARY:Bad rule\r\nDTSTART:20261009T100000Z\r\nRRULE:FREQ=SOMETIMES\r\nEND:VEVENT\r\n",
        );
        let events = events_between(&text, utc("2026-10-09T00:00:00Z"), utc("2026-10-10T00:00:00Z"), Tz::LOCAL);
        assert_eq!(events.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Bad rule"]);
    }

    #[test]
    fn places_all_day_events_in_the_given_zone() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261009\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Floating\r\nDTSTART:20261009T100000\r\nEND:VEVENT\r\n",
        );
        let window = (utc("2026-10-08T00:00:00Z"), utc("2026-10-11T00:00:00Z"));
        let moscow = events_between(&text, window.0, window.1, chrono_tz::Europe::Moscow.into());
        let yekaterinburg = events_between(&text, window.0, window.1, chrono_tz::Asia::Yekaterinburg.into());
        assert_eq!(starts(&moscow), ["2026-10-08T21:00:00+00:00", "2026-10-09T07:00:00+00:00"]);
        assert_eq!(starts(&yekaterinburg), ["2026-10-08T19:00:00+00:00", "2026-10-09T05:00:00+00:00"]);
        assert_eq!(moscow[0].end - moscow[0].start, 24 * 3600 * 1000);
    }

    #[test]
    fn reads_durations() {
        assert_eq!(parse_duration("PT1H30M"), Some(TimeDelta::minutes(90)));
        assert_eq!(parse_duration("P1W"), Some(TimeDelta::days(7)));
        assert_eq!(parse_duration("-P1DT2H"), Some(-TimeDelta::hours(26)));
        assert_eq!(parse_duration("P1H"), None);
        assert_eq!(parse_duration("PT99999999999999999H"), None);
        assert_eq!(parse_duration("1H"), None);
    }

    #[test]
    fn keeps_the_earliest_events_of_a_crowded_window() {
        // Written latest first, so the earliest ones come last.
        let events: String = (0..MAX_EVENTS + 500)
            .rev()
            .map(|i| {
                let at = utc("2026-10-09T00:00:00Z") + TimeDelta::minutes(i as i64);
                format!("BEGIN:VEVENT\r\nUID:{i}\r\nSUMMARY:E{i}\r\nDTSTART:{}\r\nEND:VEVENT\r\n", at.format("%Y%m%dT%H%M%SZ"))
            })
            .collect();
        let found = events_between(&calendar(&events), utc("2026-10-08T00:00:00Z"), utc("2026-10-12T00:00:00Z"), Tz::UTC);
        assert_eq!(found.len(), MAX_EVENTS);
        assert_eq!(found[0].title, "E0");
        assert_eq!(found[MAX_EVENTS - 1].title, format!("E{}", MAX_EVENTS - 1));
    }

    #[test]
    fn bounds_feeds_of_events_repeating_every_minute() {
        let title = "x".repeat(1024);
        let events: String = (0..5_000)
            .map(|i| {
                format!(
                    "BEGIN:VEVENT\r\nUID:{i}\r\nSUMMARY:{title}\r\nDTSTART:20261008T000000Z\r\nRRULE:FREQ=MINUTELY\r\nEND:VEVENT\r\n"
                )
            })
            .collect();
        let started = std::time::Instant::now();
        let found = events_between(&calendar(&events), utc("2026-10-08T00:00:00Z"), utc("2026-10-12T00:00:00Z"), Tz::UTC);
        assert!(started.elapsed() < std::time::Duration::from_secs(30), "took {:?}", started.elapsed());
        assert_eq!(found.len(), MAX_EVENTS);
        assert!(found.windows(2).all(|pair| pair[0].start <= pair[1].start));
        assert_eq!(found[0].title.chars().count(), MAX_TEXT_CHARS + 1);
        assert!(found[0].title.ends_with('…'));
    }

    fn budget() -> Budget {
        Budget { steps: MAX_TOTAL_STEPS, deadline: Instant::now() + MAX_EXPANSION_TIME }
    }

    #[test]
    fn skipping_ahead_keeps_the_occurrences() {
        // Around the ends of daylight saving in Europe (October 25) and the US (November 1).
        let window = (utc("2026-10-20T00:00:00Z"), utc("2026-11-05T00:00:00Z"));
        // The hourly rule would skip ahead to 02:30 on March 29, a time skipped in Berlin.
        let spring = (utc("2026-03-29T02:00:00Z"), utc("2026-03-31T00:00:00Z"));
        // A start in summer time puts its local time an hour later in winter.
        let winter = (utc("2026-11-10T01:00:00Z"), utc("2026-11-20T00:00:00Z"));
        let cases = [
            ("DTSTART;TZID=Europe/Berlin:20200101T093000", "FREQ=DAILY", window, true),
            ("DTSTART;TZID=Europe/Berlin:20190312T180000", "FREQ=DAILY;INTERVAL=3", window, true),
            ("DTSTART;TZID=America/New_York:20210104T090000", "FREQ=WEEKLY;INTERVAL=2", window, true),
            ("DTSTART;TZID=America/New_York:20240310T013000", "FREQ=HOURLY;INTERVAL=7", window, true),
            ("DTSTART;TZID=Europe/Moscow:20250101T000000", "FREQ=MINUTELY;INTERVAL=97", window, true),
            ("DTSTART:20180101T120000Z", "FREQ=DAILY;UNTIL=20261028T000000Z", window, true),
            ("DTSTART;TZID=Europe/Berlin:20160101T080000", "FREQ=DAILY;COUNT=4000", window, false),
            ("DTSTART;TZID=Europe/Berlin:20100104T100000", "FREQ=WEEKLY;BYDAY=MO,WE,FR", window, false),
            ("DTSTART;VALUE=DATE:20100101", "FREQ=DAILY;INTERVAL=5", window, true),
            ("DTSTART;TZID=Europe/Berlin:20240101T003000", "FREQ=HOURLY", spring, true),
            ("DTSTART;TZID=Europe/Berlin:20240101T003000", "FREQ=MINUTELY;INTERVAL=45", spring, true),
            ("DTSTART;TZID=Europe/Berlin:20240701T103000", "FREQ=HOURLY;INTERVAL=5", winter, true),
        ];
        let berlin: Tz = chrono_tz::Europe::Berlin.into();
        for (start, rule, (from, to), skips) in cases {
            let text = calendar(&format!(
                "BEGIN:VEVENT\r\nUID:a\r\n{start}\r\nDURATION:PT30M\r\nRRULE:{rule}\r\n\
                 EXDATE;TZID=Europe/Berlin:20261027T093000\r\nEND:VEVENT\r\n"
            ));
            let mut zones = Zones { local: berlin, by_name: HashMap::new() };
            let component = &components(&text, &mut zones)[0];
            let mut fast_budget = budget();
            let fast = component.starts(from, to, &mut fast_budget);
            assert_eq!(MAX_TOTAL_STEPS - fast_budget.steps < 600, skips, "{start} {rule}");
            // Expanded from the first start on, as without skipping ahead.
            let after = from - TimeDelta::minutes(30);
            let slow: Vec<_> = component
                .rule_set(rule, DateTime::<Utc>::MIN_UTC)
                .unwrap()
                .after(after.with_timezone(&Tz::UTC))
                .before(to.with_timezone(&Tz::UTC))
                .all(u16::MAX)
                .dates
                .into_iter()
                .filter(|start| *start >= after && *start < to)
                .take(MAX_OCCURRENCES)
                .collect();
            assert!(!slow.is_empty(), "{start} {rule}");
            assert_eq!(fast, slow, "{start} {rule}");
        }
    }

    #[test]
    fn bounds_the_work_of_hostile_rules() {
        let window = (utc("2026-10-05T00:00:00Z"), utc("2026-11-02T00:00:00Z"));
        let event = |rule: &str| {
            format!("BEGIN:VEVENT\r\nUID:{rule}\r\nSUMMARY:{rule}\r\nDTSTART:19000101T000000Z\r\nRRULE:{rule}\r\nEND:VEVENT\r\n")
        };
        let read = |events: &str| events_between(&calendar(events), window.0, window.1, Tz::UTC);
        let started = Instant::now();
        // Every second since 1900 skips ahead to the window and is cut there.
        let found = read(&event("FREQ=SECONDLY"));
        assert_eq!(found.len(), MAX_OCCURRENCES);
        assert_eq!(found[0].start, window.0.timestamp_millis());
        // A rule that cannot skip ahead stops after so many occurrences.
        assert!(read(&event("FREQ=SECONDLY;COUNT=4000000000")).is_empty());
        // Rules that make finding each occurrence slow are read as single events.
        for rule in ["FREQ=SECONDLY;BYHOUR=0;BYMINUTE=0;BYSECOND=0", "FREQ=YEARLY;BYHOUR=0,1,2,3,4,5,6,7,8,9,10,11;BYMINUTE=0,1,2,3,4,5,6"] {
            assert!(read(&event(rule)).is_empty(), "{rule}");
        }
        // Many of them use up the feed's budget.
        let many: String = (0..200).map(|i| event(&format!("FREQ=SECONDLY;COUNT=400000000{i}"))).collect();
        read(&many);
        assert!(started.elapsed() < std::time::Duration::from_secs(30), "took {:?}", started.elapsed());
    }

    #[test]
    fn leaves_out_recurring_events_once_the_time_is_up() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Daily\r\nDTSTART:20261001T090000Z\r\nRRULE:FREQ=DAILY\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Once\r\nDTSTART:20261009T100000Z\r\nEND:VEVENT\r\n",
        );
        let mut zones = Zones { local: Tz::UTC, by_name: HashMap::new() };
        let components = components(&text, &mut zones);
        let window = (utc("2026-10-08T00:00:00Z"), utc("2026-10-10T00:00:00Z"));
        assert_eq!(components[0].starts(window.0, window.1, &mut budget()).len(), 2);
        let mut late = Budget { steps: MAX_TOTAL_STEPS, deadline: Instant::now() };
        assert!(components[0].starts(window.0, window.1, &mut late).is_empty());
        assert_eq!(late.steps, 0);
        assert_eq!(components[1].starts(window.0, window.1, &mut late).len(), 1);
    }

    #[test]
    fn reads_at_most_so_many_events() {
        let events = "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261009T100000Z\r\nEND:VEVENT\r\n".repeat(MAX_COMPONENTS + 10);
        let mut zones = Zones { local: Tz::UTC, by_name: HashMap::new() };
        assert_eq!(components(&calendar(&events), &mut zones).len(), MAX_COMPONENTS);
    }

    #[cfg(windows)]
    #[test]
    fn understands_windows_zone_names() {
        assert_eq!(zone_by_name("Russian Standard Time").map(|zone| zone.name().to_string()).as_deref(), Some("Europe/Moscow"));
        assert!(zone_by_name("Nowhere Standard Time").is_none());
    }
}
