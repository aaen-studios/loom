import type { MetadataRoute } from "next";
import { SITE } from "@/lib/site";

export default function robots(): MetadataRoute.Robots {
  return {
    rules: {
      userAgent: "*",
      allow: "/",
      // The redirect endpoint is useless to a crawler — it is a 302 whose target
      // changes with every release — and keeping it out of the index avoids a
      // stale version number being served up as a search result.
      disallow: ["/download/latest"],
    },
    sitemap: `${SITE.url}/sitemap.xml`,
    host: SITE.url,
  };
}
