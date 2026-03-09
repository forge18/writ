import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  site: 'https://forge18.github.io',
  base: '/writ',
  integrations: [
    starlight({
      expressiveCode: {
        shiki: {
          langs: [
            {
              ...JSON.parse(
                fs.readFileSync(path.resolve(__dirname, '../extensions/vscode-writ/syntaxes/writ.tmLanguage.json'), 'utf-8')
              ),
              name: 'writ',
            },
          ],
        },
      },
      title: 'Writ',
      description: 'A statically typed scripting language designed for game developers. Embeds directly into Rust with near-zero interop cost — no marshalling, no runtime overhead.',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/forge18/writ' },
      ],
      sidebar: [
        { label: 'Home', slug: 'index' },
        { label: 'Getting Started', slug: 'guides/getting-started' },
        {
          label: 'Language',
          items: [
            { label: 'Fundamentals', slug: 'language/fundamentals' },
            { label: 'Functions', slug: 'language/functions' },
            { label: 'Control Flow', slug: 'language/control-flow' },
            { label: 'Classes & Structs', slug: 'language/classes-and-structs' },
            { label: 'Traits & Enums', slug: 'language/traits-and-enums' },
            { label: 'Generics & Casting', slug: 'language/generics' },
            { label: 'Error Handling', slug: 'language/error-handling' },
            { label: 'Collections', slug: 'language/collections' },
            { label: 'Coroutines', slug: 'language/coroutines' },
            { label: 'Modules', slug: 'language/modules' },
          ],
        },
        {
          label: 'Standard Library',
          items: [
            { label: 'Core', slug: 'stdlib/core' },
            { label: 'Math', slug: 'stdlib/math' },
            { label: 'Data', slug: 'stdlib/data' },
            { label: 'Game', slug: 'stdlib/game' },
            { label: 'System', slug: 'stdlib/system' },
          ],
        },
        {
          label: 'Advanced',
          items: [
            { label: 'Embedding in Rust', slug: 'advanced/embedding' },
            { label: 'Runtime & Memory', slug: 'advanced/runtime' },
            { label: 'Sandboxing & Security', slug: 'advanced/sandboxing' },
            { label: 'Debugging', slug: 'advanced/debugging' },
          ],
        },
        {
          label: 'Examples',
          items: [
            { label: 'Patterns', slug: 'examples/patterns' },
            { label: 'Projects', slug: 'examples/projects' },
          ],
        },
      ],
    }),
  ],
  output: 'static',
});
