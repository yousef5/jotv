/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        base: '#1a1a2e',
        sidebar: '#16213e',
        card: '#0f3460',
        accent: '#e94560',
        'accent-green': '#4ecca3',
        'accent-yellow': '#f0a500',
        'accent-purple': '#7b68ee',
      },
    },
  },
  plugins: [],
};
