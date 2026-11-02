import type { NextConfig } from "next";

// A fully static site: `npm run build` writes plain HTML/CSS/JS to out/, which any static host can serve.
// For a GitHub Pages project site (served under /<repo>/), build with BASE_PATH=/<repo>.
const basePath = process.env.BASE_PATH || "";

const nextConfig: NextConfig = {
  output: "export",
  trailingSlash: true,
  images: { unoptimized: true },
  basePath,
  // the other languages' pages are app/[locale]/, files named *.i18n.tsx (one page per catalog in messages/)
  pageExtensions: ["tsx", "ts", "i18n.tsx"],
  env: { NEXT_PUBLIC_BASE_PATH: basePath },
  // app/global-not-found.tsx is the 404 page, as each language has its own root layout
  experimental: { globalNotFound: true },
};

export default nextConfig;
