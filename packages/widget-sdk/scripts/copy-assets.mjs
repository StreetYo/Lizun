// Copies the stylesheet and the Inter font files next to the compiled SDK.
import { copyFileSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
const fonts = join(dist, 'fonts');
mkdirSync(fonts, { recursive: true });

copyFileSync(join(root, 'src', 'lizun.css'), join(dist, 'lizun.css'));

const require = createRequire(import.meta.url);
const inter = dirname(require.resolve('@fontsource-variable/inter/package.json'));
for (const subset of ['latin', 'latin-ext', 'cyrillic']) {
  const file = `inter-${subset}-wght-normal.woff2`;
  copyFileSync(join(inter, 'files', file), join(fonts, file));
}
console.log('copied lizun.css and fonts to dist/');
