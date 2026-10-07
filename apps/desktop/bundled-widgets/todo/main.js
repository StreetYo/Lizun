import { keyboard, ready, settings, storage } from '/__sdk/index.js';

/** Keeps the stored list well under the 64 KB limit of one storage value. */
const MAX_TASKS = 200;

const input = document.getElementById('add');
const list = document.getElementById('list');
const left = document.getElementById('left');
const clear = document.getElementById('clear');
const empty = document.getElementById('empty');

/** @type {{ text: string, done: boolean }[]} */
let tasks = [];
let options;

function save() {
  storage.set('tasks', tasks).catch((error) => console.warn('could not save the tasks', error));
}

/** Tasks in display order, as the settings ask. */
function visibleTasks() {
  if (options.completed === 'hidden') return tasks.filter((task) => !task.done);
  if (options.completed === 'bottom') return [...tasks.filter((t) => !t.done), ...tasks.filter((t) => t.done)];
  return tasks;
}

function render() {
  const open = tasks.filter((task) => !task.done).length;
  const done = tasks.length - open;
  left.textContent = tasks.length ? `${open} left` : '';
  clear.hidden = done === 0;

  const shown = visibleTasks();
  empty.hidden = shown.length > 0;
  empty.textContent = done && !open ? 'All done' : 'Nothing to do';
  list.replaceChildren(
    ...shown.map((task) => {
      const item = document.createElement('li');
      item.className = task.done ? 'task done' : 'task';

      const check = document.createElement('button');
      check.className = 'check';
      check.title = task.done ? 'Mark as not done' : 'Mark as done';
      check.textContent = task.done ? '✓' : '';
      check.addEventListener('click', () => {
        task.done = !task.done;
        save();
        render();
      });

      const text = document.createElement('span');
      text.className = 'text';
      text.textContent = task.text;
      text.title = task.text;

      const remove = document.createElement('button');
      remove.className = 'remove';
      remove.title = 'Delete';
      remove.textContent = '×';
      remove.addEventListener('click', () => {
        tasks = tasks.filter((t) => t !== task);
        save();
        render();
      });

      item.append(check, text, remove);
      return item;
    }),
  );
}

// Enter adds the task and keeps the field ready for the next one; Enter in an empty field, or
// Esc, hands the keyboard back to the app you were working in.
input.addEventListener('keydown', (event) => {
  if (event.key !== 'Enter') return;
  const text = input.value.trim();
  if (!text) {
    keyboard.release();
    return;
  }
  if (tasks.length >= MAX_TASKS) {
    input.value = '';
    input.placeholder = `At most ${MAX_TASKS} tasks`;
    return;
  }
  const task = { text, done: false };
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
options = context.settings;
settings.subscribe((next) => {
  options = next;
  render();
});
const stored = await storage.get('tasks');
tasks = Array.isArray(stored) ? stored : [];
render();
