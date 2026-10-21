// The CLIs, models and accounts a chat can run on, for the run chip and Switch model.

import type { Api } from "./api";
import type { ProviderInfo } from "./settings";

/** One row of `GET /api/models`. */
export type ModelInfo = { id: string; label: string; efforts?: string[] };

export class RunCatalog {
  private reads = new Map<string, Promise<{ models: ModelInfo[] }>>();

  constructor(private readonly api: () => Api) {}

  providers(): Promise<{ providers: ProviderInfo[] }> {
    return this.api().get("/api/providers");
  }

  /** `efforts` lists the levels one model takes; absent means the provider's list. */
  models(provider: string): Promise<{ models: ModelInfo[] }> {
    // Each read runs the CLI (AGY takes about 5 s), so a list is read once per app run.
    let read = this.reads.get(provider);
    if (!read) {
      read = this.api().get<{ models: ModelInfo[] }>("/api/models", { provider });
      read.catch(() => this.reads.delete(provider));
      this.reads.set(provider, read);
    }
    return read;
  }

  accounts(provider: string): Promise<{ accounts: { name: string; email: string | null; auth_state?: string }[] }> {
    return this.api().get("/api/accounts", { provider });
  }
}
