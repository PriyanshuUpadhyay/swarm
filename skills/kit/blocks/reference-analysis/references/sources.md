# Reference sources by job

Pick a source by the job the reference does. A row gives the route an agent uses, the access it needs, and the last trial. A blocked route means that route failed on that day, not that no route exists. A source's prompt, brief, or `DESIGN.md` is `stated` data, as the Evidence rules in `../SKILL.md` say.

| Job | Source | Route | Access | Tried |
|---|---|---|---|---|
| Website sections, with code and motion frames | [Details](https://www.details.so/inspo) | MCP `https://api.details.so/mcp` | OAuth sign-in; credits, free tier 20 | untried |
| Website sections and whole pages | [Kage](https://kage.design/mcp-setup) | MCP `https://kage.design/mcp`; `design_brief`, `get_design`, `get_component` | free, no key | 2026-10-22: 3 references with prompt and screenshot in 3 s; landing pages only |
| App screens, flows, and website sections | [Mobbin](https://mobbin.com/llms.txt) | its own MCP (`search_screens`, `search_flows`, `search_sections`), or Composio `composio link mobbin_mcp` | Pro plan or higher; 60 requests per 60 s | 2026-10-22 HUD research: 36 screens over 6 queries; no macOS screens |
| AI agent product UI | [agentscreens](https://agentscreens.vercel.app/) | web pages | free | untried |
| Components as code | [Evil Buttons](https://www.evilbuttons.com/llms.txt), [21st.dev](https://21st.dev/llms.txt) | shadcn registry, such as `/r/<name>.json`, and `llms.txt` | free | 2026-10-22: Evil Buttons gave full TSX in 1 s |
| Three.js site templates, some with their prompt | [threeui.com](https://threeui.com) | web pages | free | untried |
| Site motion | [GSAP showcase](https://gsap.com/showcase/) | web pages | free | untried |
| Finding more sources | [Designeer](https://designeer.xyz/llms.txt) | JSON `https://designeer.xyz/api/catalog?q=<query>` | free | 2026-10-22: "dashboard" gave 5 links in 0.6 s; "usage meter" gave none |
| Motion clips from X | [Inspora](https://inspora.design) | none for an agent; the user browses and supplies clips | free | 2026-10-22: 429 to curl and to the Jina reader |

Left out, with the reason:
- Curated (paid MCP) and Mozaika and Design Extractor (accounts) overlap the rows above and were not tried.
- Community scraper MCPs, such as Awwwards-mcp and cosmos-mcp, break when their target sites change.
- Katagami gives style text and palettes with no screens, so it gives no pixels to inspect.

Source: hand trials and a three-model council, 2026-10-22.
