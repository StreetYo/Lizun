/**
 * Days and clock times in a given IANA time zone. The WebView keeps the time zone it started
 * with, so `Date` and a default `Intl.DateTimeFormat` can lag behind a change in the Windows
 * settings; the core reports the current zone and the widgets count days in it.
 */
export class ZonedTime {
  readonly #zone: string | undefined;
  readonly #fields: Intl.DateTimeFormat;
  readonly #formats = new Map<string, Intl.DateTimeFormat>();

  /** An empty name means the WebView's own zone. */
  constructor(zone: string) {
    this.#zone = zone || undefined;
    this.#fields = new Intl.DateTimeFormat('en-US', {
      timeZone: this.#zone,
      hourCycle: 'h23',
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
      hour: 'numeric',
      minute: 'numeric',
      second: 'numeric',
    });
  }

  /** The wall-clock date and time of an instant in this zone. */
  fields(time: number): { year: number; month: number; day: number; hour: number; minute: number; second: number } {
    const parts = Object.fromEntries(this.#fields.formatToParts(time).map((part) => [part.type, Number(part.value)]));
    return { year: parts.year, month: parts.month, day: parts.day, hour: parts.hour % 24, minute: parts.minute, second: parts.second };
  }

  /** Midnight that starts the day `addDays` days after the day of `time`: the first instant of
   * that day, 01:00 where daylight saving starts at midnight. */
  midnight(time: number, addDays = 0): number {
    const { year, month, day } = this.fields(time);
    const wall = Date.UTC(year, month - 1, day + addDays);
    const target = new Date(wall);
    // The offset at a first guess, then again at the result, gives the offsets on both sides of
    // a daylight saving change. The earlier of the two that falls on the day starts it; where
    // midnight is skipped, only the one after the change does.
    const first = wall - this.#offset(wall);
    const candidates = [first, wall - this.#offset(first)].sort((a, b) => a - b);
    const onDay = (at: number) => {
      const f = this.fields(at);
      return f.year === target.getUTCFullYear() && f.month === target.getUTCMonth() + 1 && f.day === target.getUTCDate();
    };
    return candidates.find(onDay) ?? candidates[1];
  }

  format(time: number, options: Intl.DateTimeFormatOptions, locale?: string): string {
    const key = JSON.stringify([locale ?? null, options]);
    let format = this.#formats.get(key);
    if (!format) {
      format = new Intl.DateTimeFormat(locale, { ...options, timeZone: this.#zone });
      this.#formats.set(key, format);
    }
    return format.format(time);
  }

  /** How far this zone's clock is ahead of UTC at `time`, in milliseconds. */
  #offset(time: number): number {
    const f = this.fields(time);
    return Date.UTC(f.year, f.month - 1, f.day, f.hour, f.minute, f.second) - Math.floor(time / 1000) * 1000;
  }
}
