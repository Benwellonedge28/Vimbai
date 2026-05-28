/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        primary: { DEFAULT: '#4f46e5', 600: '#4338ca', 700: '#3730a3' },
      },
    },
  },
  plugins: [],
}
