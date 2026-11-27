// The CLIs, models and accounts a chat can run on, for the run chip and Switch model.

import type { Api } from "./api";
import type { AccountList, ProviderInfo } from "./settings";

/** One row of `GET /api/models`. `note` is a short line beside the name; `default_effort` the
 *  level the CLI picks for it. */
export type ModelInfo = { id: string; label: string; efforts?: string[]; note?: string; default_effort?: string };

type ModelRead = { read: Promise<{ models: ModelInfo[] }>; models?: ModelInfo[] };

export class RunCatalog {
  private reads = new Map<string, ModelRead>();
  /** Each account's email from the last accounts read, by `provider/name`, for the chip's tooltip. */
  private emails = new Map<string, string>();

  constructor(private readonly api: () => Api) {}

  providers(): Promise<{ providers: ProviderInfo[] }> {
    return this.api().get("/api/providers");
  }

  /** `efforts` lists the levels one model takes; absent means the provider's list. */
  models(provider: string): Promise<{ models: ModelInfo[] }> {
    // Each read runs the CLI (AGY takes about 5 s), so a list is read once per app run.
    const cached = this.reads.get(provider);
    if (cached) return cached.read;

    const entry: ModelRead = { read: this.api().get("/api/models", { provider }) };
    entry.read.then(({ models }) => { entry.models = models; }, () => this.reads.delete(provider));
    this.reads.set(provider, entry);
    return entry.read;
  }

  loadedModels(provider: string): readonly ModelInfo[] | undefined {
    return this.reads.get(provider)?.models;
  }

  async accounts(provider: string): Promise<AccountList> {
    const read = await this.api().get<AccountList>("/api/accounts", { provider });
    for (const key of this.emails.keys()) if (key.startsWith(`${provider}/`)) this.emails.delete(key);
    for (const a of read.accounts) if (a.email) this.emails.set(`${provider}/${a.name}`, a.email);
    return read;
  }

  /** The account's email, once the popover read its CLI's accounts; else null. */
  email(provider: string, account: string): string | null {
    return this.emails.get(`${provider}/${account}`) ?? null;
  }
}
