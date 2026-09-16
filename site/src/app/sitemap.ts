import type { MetadataRoute } from "next";
import { SITE } from "@/lib/site";

/**
 * The four indexable URLs.
 *
 * `/download/latest` is deliberately absent. It is a route handler that only ever
 * emits a 302, and listing it would invite crawlers to follow a URL whose target
 * changes with every release — the one kind of page that should never be cached
 * under a stable name.
 */
export default function sitemap(): MetadataRoute.Sitemap {
  return [
    {
      url: SITE.url,
      lastModified: new Date(),
      changeFrequency: "weekly",
      priority: 1,
    },
    {
      url: `${SITE.url}/download`,
      lastModified: new Date(),
      changeFrequency: "weekly",
      priority: 0.9,
    },
    {
      url: `${SITE.url}/privacy`,
      lastModified: new Date(),
      changeFrequency: "yearly",
      priority: 0.3,
    },
    {
      url: `${SITE.url}/terms`,
      lastModified: new Date(),
      changeFrequency: "yearly",
      priority: 0.3,
    },
  ];
}
