import type { MetadataRoute } from "next";
import { SITE } from "@/lib/site";

/**
 * The four indexable URLs. The redirect at `/download/latest` is deliberately
 * absent: it is a route handler that only ever sends a 302, and listing it
 * would invite crawlers to follow a URL whose target changes with every
 * release.
 */
export default function sitemap(): MetadataRoute.Sitemap {
  const now = new Date();

  return [
    { url: SITE.url, lastModified: now, changeFrequency: "weekly", priority: 1 },
    {
      url: `${SITE.url}/download`,
      lastModified: now,
      changeFrequency: "weekly",
      priority: 0.9,
    },
    {
      url: `${SITE.url}/privacy`,
      lastModified: now,
      changeFrequency: "yearly",
      priority: 0.3,
    },
    {
      url: `${SITE.url}/terms`,
      lastModified: now,
      changeFrequency: "yearly",
      priority: 0.3,
    },
  ];
}
