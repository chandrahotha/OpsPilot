/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        primary: {
          DEFAULT: '#3b82f6',
          dark: '#2563eb',
        },
        background: '#0f172a',
        surface: '#1e293b',
        'surface-hover': '#334155',
        border: '#334155',
      },
    },
  },
  plugins: [],
}