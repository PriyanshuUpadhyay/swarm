// Copied from jean (https://github.com/coollabsio/jean, src/lib/fuzzy-search.ts at bda4576e).
// Only `fuzzySearchItems` is kept; the palette and the composer's `/` menu call it.
//
// Copyright 2025 Andras Bacsai
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import Fuse from "fuse.js";

/**
 * Fuzzy search over slash commands and skills by name and description.
 */
export function fuzzySearchItems<T extends { name: string; description?: string | null }>(items: T[], query: string, limit = 15): T[] {
  if (!query) return items.slice(0, limit);

  const fuse = new Fuse(items, {
    keys: [
      { name: "name", weight: 2 },
      { name: "description", weight: 1 },
    ],
    threshold: 0.4,
    ignoreLocation: true,
  });

  return fuse.search(query, { limit }).map((r) => r.item);
}
