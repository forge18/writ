import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://forge18.github.io',
  base: '/writ',
  integrations: [
    starlight({
      title: 'Writ',
      description: 'A statically typed, embedded scripting language designed for games and applications',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/forge18/writ' },
      ],
      sidebar: [
        { label: 'Home', slug: 'index' },
        {
          label: 'Reference',
          items: [
            'reference/language-spec',
          ],
        },
      ],
    }),
  ],
  output: 'static',
});
