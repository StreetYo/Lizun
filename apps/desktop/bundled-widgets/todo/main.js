import { activity, keyboard, language, ready, settings, storage } from '/__sdk/index.js';

/** Keeps the stored list well under the 64 KB limit of one storage value. */
const MAX_TASKS = 200;

/** The widget's own texts by language; the manifest translates its name and settings. */
const TEXTS = {
  en: {
    title: 'To-do',
    add: 'Add a task',
    left: (count) => `${count} left`,
    clear: 'Clear done',
    empty: 'Nothing to do',
    allDone: 'All done',
    markDone: 'Mark as done',
    markNotDone: 'Mark as not done',
    remove: 'Delete',
    full: `At most ${MAX_TASKS} tasks`,
  },
  ru: {
    title: 'Задачи',
    add: 'Добавить задачу',
    left: (count) => `осталось ${count}`,
    clear: 'Убрать готовые',
    empty: 'Дел нет',
    allDone: 'Всё сделано',
    markDone: 'Отметить как сделанную',
    markNotDone: 'Отметить как несделанную',
    remove: 'Удалить',
    full: `Не больше ${MAX_TASKS} задач`,
  },
};

/** Texts in Lizun's language, or in English for a language the widget does not know. */
let text = TEXTS.en;

const title = document.getElementById('title');
const input = document.getElementById('add');
const list = document.getElementById('list');
const left = document.getElementById('left');
const clear = document.getElementById('clear');
const empty = document.getElementById('empty');

/** @type {{ text: string, done: boolean }[]} */
let tasks = [];
let options;
/** Color for secondary text, from the island theme. */
let secondary;

function save() {
  storage.set('tasks', tasks).catch((error) => console.warn('could not save the tasks', error));
}

/** Tasks in display order, as the settings ask. */
function visibleTasks() {
  if (options.completed === 'hidden') return tasks.filter((task) => !task.done);
  if (options.completed === 'bottom') return [...tasks.filter((t) => !t.done), ...tasks.filter((t) => t.done)];
  return tasks;
}

/** What the closed island shows, to skip sending the same activity again. */
let shownActivity;

/** The next open task in the closed island, and how many more are left. */
function showActivity() {
  const open = tasks.filter((task) => !task.done);
  const next =
    options.showInIsland && open.length
      ? {
          leading: { icon: 'list-checks' },
          title: open[0].text.slice(0, 100),
          trailing: open.length > 1 ? { text: `+${open.length - 1}`, color: secondary } : undefined,
        }
      : null;
  const key = JSON.stringify(next);
  if (key === shownActivity) return;
  shownActivity = key;
  const update = next ? activity.set(next) : activity.clear();
  update.catch((error) => console.warn('could not update the live activity', error));
}

function render() {
  showActivity();
  const open = tasks.filter((task) => !task.done).length;
  const done = tasks.length - open;
  title.textContent = text.title;
  clear.textContent = text.clear;
  input.placeholder = tasks.length >= MAX_TASKS ? text.full : text.add;
  left.textContent = tasks.length ? text.left(open) : '';
  clear.hidden = done === 0;

  const shown = visibleTasks();
  empty.hidden = shown.length > 0;
  empty.textContent = done && !open ? text.allDone : text.empty;
  list.replaceChildren(
    ...shown.map((task) => {
      const item = document.createElement('li');
      item.className = task.done ? 'task done' : 'task';

      const check = document.createElement('button');
      check.className = 'check';
      check.title = task.done ? text.markNotDone : text.markDone;
      check.textContent = task.done ? '✓' : '';
      check.addEventListener('click', () => {
        task.done = !task.done;
        save();
        render();
      });

      const label = document.createElement('span');
      label.className = 'text';
      label.textContent = task.text;
      label.title = task.text;

      const remove = document.createElement('button');
      remove.className = 'remove';
      remove.title = text.remove;
      remove.textContent = '×';
      remove.addEventListener('click', () => {
        tasks = tasks.filter((t) => t !== task);
        save();
        render();
      });

      item.append(check, label, remove);
      return item;
    }),
  );
}

// Enter adds the task and keeps the field ready for the next one; Enter in an empty field, or
// Esc, hands the keyboard back to the app you were working in.
input.addEventListener('keydown', (event) => {
  if (event.key !== 'Enter') return;
  const value = input.value.trim();
  if (!value) {
    keyboard.release();
    return;
  }
  if (tasks.length >= MAX_TASKS) {
    input.value = '';
    input.placeholder = text.full;
    return;
  }
  const task = { text: value, done: false };
  tasks = options.newOnTop ? [task, ...tasks] : [...tasks, task];
  input.value = '';
  save();
  render();
});

clear.addEventListener('click', () => {
  tasks = tasks.filter((task) => !task.done);
  save();
  render();
});

const context = await ready();
text = TEXTS[context.language] ?? TEXTS.en;
language.subscribe((next) => {
  text = TEXTS[next] ?? TEXTS.en;
  render();
});
options = context.settings;
secondary = context.theme.textSecondary;
settings.subscribe((next) => {
  options = next;
  render();
});
const stored = await storage.get('tasks');
tasks = Array.isArray(stored) ? stored : [];
render();
