/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        base: '#141313',
        sidebar: '#0e0d0d',
        card: '#232020',
        accent: '#e50914',
        'accent-green': '#4ecca3',
        'accent-yellow': '#f0a500',
        'accent-purple': '#7b68ee',
      },
    },
  },
  plugins: [],
};
