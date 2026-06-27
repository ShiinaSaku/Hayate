import * as path from 'node:path';
import { defineConfig } from '@rspress/core';

export default defineConfig({
  root: path.join(__dirname, 'docs'),
  lang: 'en',
  title: 'Hayate',
  description: 'Swift, Secure, Encrypted, & Compressed Local File Transfer over QUIC',
  icon: '/logo.svg',
  logo: {
    light: '/logo.svg',
    dark: '/logo.svg',
  },
  base: '/Hayate/',
  sitemap: {
    hostname: 'https://shiinasaku.github.io/Hayate/',
  },
  llms: true,
  globalStyles: path.join(__dirname, 'styles/custom.css'),
  themeConfig: {
    socialLinks: [
      {
        icon: 'github',
        mode: 'link',
        content: 'https://github.com/ShiinaSaku/Hayate',
      },
    ],
  },
});
