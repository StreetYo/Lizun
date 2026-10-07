<script lang="ts">
  import CalendarBlank from 'phosphor-svelte/lib/CalendarBlank';

  // Events are not connected yet; the widget shows the week around today.
  const today = new Date();
  const month = today.toLocaleString('en', { month: 'short' });
  const days = Array.from({ length: 7 }, (_, i) => {
    const date = new Date(today);
    date.setDate(today.getDate() + i - 2);
    return {
      key: date.toDateString(),
      day: String(date.getDate()).padStart(2, '0'),
      weekday: date.toLocaleString('en', { weekday: 'short' }),
      isToday: i === 2,
      isPast: i < 2,
    };
  });
</script>

<section class="calendar">
  <div class="head">
    <span class="month">{month}</span>
    <div class="week">
      {#each days as day (day.key)}
        <div class="day" class:today={day.isToday} class:past={day.isPast}>
          <span class="weekday">{day.isToday ? day.weekday.toUpperCase() : day.weekday[0]}</span>
          <span class="number">{day.day}</span>
        </div>
      {/each}
    </div>
  </div>
  <div class="empty">
    <CalendarBlank size={18} weight="fill" />
    <span>Nothing for today</span>
  </div>
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
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.1;
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

  .today .weekday {
    color: var(--text);
  }

  .today .number {
    font-size: 20px;
    font-weight: 700;
    color: var(--accent);
  }

  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    font-weight: 500;
    color: var(--text-secondary);
  }
</style>
