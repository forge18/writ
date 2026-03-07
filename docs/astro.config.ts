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
        {
          label: 'Guides',
          items: [
            { label: 'Getting Started', slug: 'guides/getting-started' },
            { label: 'Embedding in Rust', slug: 'guides/embedding' },
          ],
        },
        {
          label: 'Language',
          items: [
            { label: 'Basics', slug: 'language/basics' },
            { label: 'Types', slug: 'language/types' },
            { label: 'Coroutines', slug: 'language/coroutines' },
            { label: 'Modules', slug: 'language/modules' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Standard Library', slug: 'reference/stdlib' },
            { label: 'Language Spec', slug: 'reference/language-spec' },
          ],
        },
      ],
    }),
  ],
  output: 'static',
});
