<script lang="ts">
  import CalendarBlank from 'phosphor-svelte/lib/CalendarBlank';
  import CircleNotch from 'phosphor-svelte/lib/CircleNotch';
  import Warning from 'phosphor-svelte/lib/Warning';
  import { calendar, type CalendarEvent } from '../../lib/calendar.svelte';
  import { i18n, t, type MessageKey } from '../../lib/i18n.svelte';
  import { invoke } from '../../lib/platform';
  import { ui } from '../../lib/ui.svelte';
  import { widgetSettings } from '../../lib/widgets/settings.svelte';
  import { ZonedTime } from '../../lib/zonedTime';

  /** Rows that fit under the week; with more events the last row says how many are left. */
  const MAX_ROWS = 4;
  const MINUTE = 60_000;
  const DAY = 24 * 60 * MINUTE;

  let now = $state(Date.now());
  /** The listed day, in days from today. */
  let offset = $state(0);
  /** Key of the event whose time the pointer is on; its countdown is shown. */
  let hovered = $state<string | null>(null);

  // The clock moves on only while the island is open. Opening it also goes back to today, and
  // asking the core again brings a stale feed or a changed time zone up to date.
  $effect(() => {
    if (!ui.expanded) return;
    now = Date.now();
    offset = 0;
    void calendar.refresh();
    const timer = setInterval(() => {
      now = Date.now();
      void calendar.refresh();
    }, 30_000);
    return () => clearInterval(timer);
  });

  // The countdown under the pointer counts every second.
  $effect(() => {
    if (!hovered) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  // Days and times follow the system time zone the core reports, not the WebView's.
  const zone = $derived(new ZonedTime(calendar.snapshot.timeZone));

  function overlaps(event: CalendarEvent, start: number, end: number): boolean {
    return event.end > event.start ? event.end > start && event.start < end : event.start >= start && event.start < end;
  }

  const events = $derived(calendar.snapshot.events);
  const days = $derived(
    Array.from({ length: 7 }, (_, i) => {
      const start = zone.midnight(now, i - 2);
      const end = zone.midnight(now, i - 1);
      return {
        start,
        end,
        offset: i - 2,
        day: String(zone.fields(start).day).padStart(2, '0'),
        weekday: zone.format(start, { weekday: 'short' }, i18n.language),
        hasEvents: events.some((event) => overlaps(event, start, end)),
      };
    }),
  );
  const selected = $derived(days[offset + 2]);
  const month = $derived(zone.format(selected.start, { month: 'short' }, i18n.language));
  const onDay = $derived(events.filter((event) => overlaps(event, selected.start, selected.end)));
  /** The "Events that ended today" setting: `dim` or `hide`. */
  const hideEnded = $derived(widgetSettings.values['builtin:calendar']?.endedToday === 'hide');

  function ended(event: CalendarEvent): boolean {
    return !event.allDay && event.end <= now;
  }

  // Today lists what is still ahead first, then what has ended, the latest first, so the rows
  // that fit go to what matters next. Ended ones can be left out in the settings.
  const listed = $derived.by(() => {
    if (selected.offset !== 0) return onDay;
    const ahead = onDay.filter((event) => !ended(event));
    return hideEnded ? ahead : [...ahead, ...onDay.filter(ended).sort((a, b) => b.end - a.end)];
  });
  const shown = $derived(listed.length > MAX_ROWS ? listed.slice(0, MAX_ROWS - 1) : listed);
  const firstLoad = $derived(calendar.snapshot.updatedAt === null);

  function timeLabel(event: CalendarEvent): string {
    if (event.allDay || (event.start <= selected.start && event.end >= selected.end)) return t('calendar.allDay');
    const time = (at: number) => zone.format(at, { hour: 'numeric', minute: '2-digit' }, i18n.language);
    if (event.start < selected.start) return `–${time(event.end)}`;
    return time(event.start);
  }

  function eventKey(event: CalendarEvent): string {
    return `${event.start}-${event.title}`;
  }

  /** `2 d 3 h`, `1 h 5 min`, `12 min`; rounded up for what is ahead, so it never says 0. Null
   * under a minute, which each phrase words on its own. */
  function span(ms: number, ahead: boolean): string | null {
    if (ms < MINUTE) return null;
    const minutes = ahead ? Math.ceil(ms / MINUTE) : Math.floor(ms / MINUTE);
    const days = Math.floor(minutes / 1440);
    const hours = Math.floor((minutes % 1440) / 60);
    const rest = minutes % 60;
    const unit = (key: 'days' | 'hours' | 'minutes', count: number) => count && t(`calendar.${key}`, { count });
    const parts = days
      ? [unit('days', days), unit('hours', hours)]
      : hours
        ? [unit('hours', hours), unit('minutes', rest)]
        : [t('calendar.minutes', { count: rest })];
    return parts.filter(Boolean).join(' ');
  }

  /** How far the event is from now, for the hint over its time. */
  function countdown(event: CalendarEvent): string {
    if (event.allDay) {
      // Whole days, counted between midnights.
      const days = (at: number) => Math.round((zone.midnight(at) - zone.midnight(now)) / DAY);
      if (event.start > now) {
        const ahead = days(event.start);
        return ahead === 1 ? t('calendar.tomorrow') : t('calendar.inDays', { count: ahead });
      }
      if (event.end > now) return t('calendar.today');
      const ago = -days(event.end - 1);
      return ago <= 1 ? t('calendar.yesterday') : t('calendar.daysAgo', { count: ago });
    }
    const phrase = (time: string | null, some: MessageKey, none: MessageKey) => (time ? t(some, { time }) : t(none));
    if (event.start > now) return phrase(span(event.start - now, true), 'calendar.startsIn', 'calendar.startsSoon');
    if (event.end > now) {
      return t('calendar.running', {
        started: phrase(span(now - event.start, false), 'calendar.startedAgo', 'calendar.justStarted'),
        ends: phrase(span(event.end - now, true), 'calendar.endsIn', 'calendar.endsSoon'),
      });
    }
    return phrase(span(now - event.end, false), 'calendar.endedAgo', 'calendar.justEnded');
  }

  const emptyText = $derived(
    t(selected.offset !== 0 ? 'calendar.noEvents' : onDay.length ? 'calendar.nothingElseToday' : 'calendar.nothingToday'),
  );

  function openSettings(): void {
    invoke('settings_open').catch((err) => console.warn('settings', err));
  }
</script>

<section class="calendar">
  <div class="head">
    <span class="month">
      {month}
      {#if calendar.snapshot.error && !firstLoad}
        <span class="stale" title={calendar.snapshot.error}><Warning size={13} weight="fill" /></span>
      {/if}
    </span>
    <div class="week">
      {#each days as day (day.start)}
        <button
          class="day"
          class:today={day.offset === 0}
          class:past={day.offset < 0}
          class:selected={day.offset === offset}
          onclick={() => (offset = day.offset)}
        >
          <span class="weekday">{day.offset === 0 ? day.weekday.toUpperCase() : day.weekday[0]}</span>
          <span class="number">{day.day}</span>
          <span class="dot" class:on={day.hasEvents}></span>
        </button>
      {/each}
    </div>
  </div>

  {#if !calendar.snapshot.configured}
    <button class="empty action" onclick={openSettings} title={t('calendar.settingsPath')}>
      <CalendarBlank size={18} weight="fill" />
      <span>{t('calendar.addSource')}</span>
      <span class="hint">{t('calendar.inSettings')}</span>
    </button>
  {:else if firstLoad && calendar.snapshot.error}
    <div class="empty error">
      <Warning size={18} weight="fill" />
      <span class="hint">{calendar.snapshot.error}</span>
    </div>
  {:else if firstLoad}
    <div class="empty">
      <span class="spin"><CircleNotch size={18} weight="bold" /></span>
      <span>{t('calendar.loading')}</span>
    </div>
  {:else if listed.length}
    <ul class="events">
      {#each shown as event (eventKey(event))}
        {@const key = eventKey(event)}
        <li
          class="event"
          class:all-day={event.allDay}
          class:now={!event.allDay && event.start <= now && event.end > now}
          class:ended={ended(event)}
        >
          <span
            class="time"
            role="presentation"
            onpointerenter={() => (hovered = key)}
            onpointerleave={() => hovered === key && (hovered = null)}
          >{timeLabel(event)}</span>
          <span class="title" title={[event.title, event.location].filter(Boolean).join(' · ')}>
            {event.title || t('calendar.untitled')}
          </span>
          {#if hovered === key}
            <span class="countdown" role="tooltip">{countdown(event)}</span>
          {/if}
        </li>
      {/each}
      {#if shown.length < listed.length}
        <li class="more">{t('calendar.more', { count: listed.length - shown.length })}</li>
      {/if}
    </ul>
  {:else}
    <div class="empty">
      <CalendarBlank size={18} weight="fill" />
      <span>{emptyText}</span>
    </div>
  {/if}
</section>

<style>
  .calendar {
    display: flex;
    flex-direction: column;
    width: 300px;
    padding-top: 4px;
  }

  .head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .month {
    position: relative;
    min-width: 52px;
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.1;
  }

  .stale {
    position: absolute;
    top: -2px;
    right: -10px;
    color: var(--text-tertiary);
  }

  .week {
    display: flex;
    gap: 6px;
    mask-image: linear-gradient(90deg, transparent, #000 12%, #000 88%, transparent);
  }

  .day {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 28px;
    padding-bottom: 3px;
    border-radius: 8px;
    transition: background 150ms ease;
  }

  .day:hover,
  .day.selected:not(.today) {
    background: var(--card);
  }

  .weekday {
    font-size: 9.5px;
    font-weight: 600;
    color: var(--text-tertiary);
  }

  .number {
    font-size: 17px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .past .number {
    color: var(--text-tertiary);
  }

  .today .weekday,
  .selected .number {
    color: var(--text);
  }

  .today .number {
    font-size: 20px;
    font-weight: 700;
    color: var(--accent);
  }

  .dot {
    width: 4px;
    height: 4px;
    margin-top: 1px;
    border-radius: 50%;
  }

  .dot.on {
    background: var(--text-tertiary);
  }

  .today .dot.on {
    background: var(--accent);
  }

  .events {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  .event {
    position: relative;
    display: grid;
    grid-template-columns: 52px 3px 1fr;
    align-items: center;
    gap: 8px;
    height: 22px;
    font-size: 13px;
  }

  .event::before {
    content: '';
    grid-column: 2;
    grid-row: 1;
    height: 14px;
    border-radius: 2px;
    background: var(--accent);
  }

  .event.all-day::before {
    background: var(--text-tertiary);
  }

  .event.ended::before {
    background: var(--text-tertiary);
    opacity: 0.6;
  }

  .ended .time,
  .ended .title {
    color: var(--text-tertiary);
  }

  .time {
    grid-column: 1;
    grid-row: 1;
    font-size: 11.5px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .now .time {
    color: var(--accent);
  }

  .time:hover {
    color: var(--text);
  }

  .countdown {
    position: absolute;
    bottom: calc(100% + 2px);
    left: -6px;
    z-index: 2;
    padding: 3px 8px;
    border-radius: 8px;
    background: var(--card-raised);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.5);
    font-size: 11.5px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
    animation: appear 120ms ease-out;
  }

  @keyframes appear {
    from {
      opacity: 0;
      translate: 0 2px;
    }
  }

  .title {
    grid-column: 3;
    grid-row: 1;
    min-width: 0;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .more {
    padding-left: 63px;
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    max-width: 260px;
    font-size: 14px;
    font-weight: 500;
    color: var(--text-secondary);
    text-align: center;
  }

  .empty.action:hover {
    color: var(--text);
  }

  .hint {
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .error .hint {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    overflow: hidden;
  }

  .spin {
    display: flex;
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
</style>
