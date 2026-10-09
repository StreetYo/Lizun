import { invoke, listen } from './platform';

/** One occurrence of an event (see `Event` in src-tauri/src/ics.rs). */
export interface CalendarEvent {
  title: string;
  location: string;
  /** Milliseconds since the epoch; all-day events run from local midnight to local midnight. */
  start: number;
  end: number;
  allDay: boolean;
}

/** What the core knows about the calendar feed (see src-tauri/src/calendar.rs). */
export interface CalendarSnapshot {
  configured: boolean;
  loading: boolean;
  events: CalendarEvent[];
  error: string | null;
  updatedAt: number | null;
  /** IANA name of the system time zone; empty when unknown. */
  timeZone: string;
}

const EMPTY: CalendarSnapshot = { configured: false, loading: false, events: [], error: null, updatedAt: null, timeZone: '' };

/** Events from the calendar link set in the widget's settings, downloaded by the core. */
class CalendarStore {
  snapshot = $state<CalendarSnapshot>(EMPTY);

  #started = false;

  async start(): Promise<void> {
    if (this.#started) return;
    this.#started = true;
    await listen<CalendarSnapshot>('calendar://changed', (snapshot) => (this.snapshot = snapshot));
    await this.refresh();
  }

  /** Reads the events again; the core also downloads a feed that is older than its refresh interval. */
  async refresh(): Promise<void> {
    try {
      this.snapshot = (await invoke<CalendarSnapshot>('calendar_snapshot')) ?? EMPTY;
    } catch (err) {
      console.warn('could not read the calendar', err);
    }
  }
}

export const calendar = new CalendarStore();
