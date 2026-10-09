import '@fontsource-variable/inter';
import '../app.css';
import { mount } from 'svelte';
import { i18n } from '../lib/i18n.svelte';
import Settings from './Settings.svelte';

// Rendered once the language is known, so the window never shows English first.
await i18n.start().catch((err) => console.warn('could not load the language', err));
const app = mount(Settings, { target: document.getElementById('app')! });

export default app;
