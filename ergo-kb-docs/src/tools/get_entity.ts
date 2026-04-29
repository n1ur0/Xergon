import { loadIndex } from "./search_docs.ts";

/**
 * get_entity — Look up any ecosystem project or wiki entity by name.
 * Searches across ergodocs:eco/* and wiki:entities/* with fuzzy matching.
 *
 * Examples:
 *   get_entity(name="ergo-mcp")
 *   get_entity(name="Dexy Peg Bots")
 *   get_entity(name="xergon")
 *   get_entity(name="yolo chain")
 */
export async function getEntity(name: string): Promise<string> {
  const index = await loadIndex();
  const queryLower = name.trim().toLowerCase();

  // Collect project/entity docs from both sources
  const candidates: Array<{
    doc: NonNullable<typeof index.documents[number]>;
    score: number;
  }> = [];

  for (const doc of index.documents) {
    let isProject = false;
    const inErgoDocs = doc.source === "ergodocs" && doc.id.startsWith("ergodocs:eco/");
    const inWiki = doc.source === "wiki" && doc.id.startsWith("wiki:entities/");

    if (!inErgoDocs && !inWiki) continue;
    isProject = true;

    if (!isProject) continue;

    // Exact match on id (slug)
    const slug = doc.id.split("/").pop() ?? "";
    const title = doc.title ?? "";

    // Calculate match score
    let score = 0;

    // Exact match on id/slug (highest priority)
    if (slug === queryLower || slug.replace(/-/g, "") === queryLower.replace(/-/g, "")) {
      score = 1000;
    }
    // Exact match on title
    else if (title.toLowerCase() === queryLower) {
      score = 900;
    }
    // Title contains query
    else if (title.toLowerCase().includes(queryLower)) {
      score = 500 + (queryLower.length * 10);
    }
    // Slug contains query
    else if (slug.includes(queryLower.replace(/-/g, ""))) {
      score = 400;
    }
    // Title contains any word from query
    else {
      const words = queryLower.split(/\s+/);
      let matchCount = 0;
      for (const word of words) {
        if (word.length < 2) continue;
        if (title.toLowerCase().includes(word)) matchCount++;
        else if (slug.replace(/-/g, "").includes(word)) matchCount++;
      }
      if (matchCount > 0) {
        score = 100 + (matchCount * 100);
      }
    }

    if (score > 0) {
      candidates.push({ doc, score });
    }
  }

  // Sort by score descending
  candidates.sort((a, b) => b.score - a.score);

  if (candidates.length === 0) {
    return JSON.stringify({
      error: `No entity found matching "${name}".`,
      hint: "Try list_projects to see all available entities, or use search_docs for broader search.",
    }, null, 2);
  }

  // Return the best match (with top alternatives if close in score)
  const best = candidates[0]!;
  const alternatives = candidates.slice(1).filter((c) => c.score >= best.score * 0.5);

  const result = {
    found: true,
    name: name,
    match: {
      id: best.doc.id,
      source: best.doc.source,
      title: best.doc.title,
      category: best.doc.category,
      tags: best.doc.tags,
      summary: best.doc.summary,
      headings: best.doc.headings,
      content: best.doc.content,
      score: best.score,
    },
    alternatives: alternatives.map((c) => ({
      id: c.doc.id,
      title: c.doc.title,
      source: c.doc.source,
      score: c.score,
    })),
  };

  return JSON.stringify(result, null, 2);
}
