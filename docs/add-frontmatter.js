import fs from 'fs';
import path from 'path';

const docsDir = './src/content/docs';

function addFrontmatter(dir) {
  const entries = fs.readdirSync(dir);

  for (const entry of entries) {
    const fullPath = path.join(dir, entry);
    const stat = fs.statSync(fullPath);

    if (stat.isDirectory()) {
      addFrontmatter(fullPath);
    } else if (entry.endsWith('.md') && entry !== 'SUMMARY.md') {
      const content = fs.readFileSync(fullPath, 'utf-8');

      // Skip if already has frontmatter
      if (content.startsWith('---')) {
        continue;
      }

      // Extract title from first heading
      const headingMatch = content.match(/^#+\s+(.+)$/m);
      const title = headingMatch ? headingMatch[1] : entry.replace('.md', '').replace(/-/g, ' ');

      const frontmatter = `---
title: ${title}
---

`;

      fs.writeFileSync(fullPath, frontmatter + content);
      console.log(`Added frontmatter to ${fullPath}`);
    }
  }
}

addFrontmatter(docsDir);
