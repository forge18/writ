import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://forge18.github.io',
  base: '/writ',
  integrations: [
    starlight({
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
            { label: 'Types', slug: 'language/types' },
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
