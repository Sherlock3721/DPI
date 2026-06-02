/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{svelte,js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        labdark: "#0b0f19",
        labcard: "#121926",
        labaccent: "#3b82f6",
        labgreen: "#10b981",
        labred: "#ef4444",
      }
    },
  },
  plugins: [],
}
