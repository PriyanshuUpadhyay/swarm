import { Component, type ReactNode } from "react";

/** Catches a failed load or render of a lazy part, so the rest of the page stays. Key it by what
 *  it shows, so the next one starts clean. */
export class LoadFailed extends Component<{ what: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    return this.state.failed ? (
      <p role="alert" className="p-3 text-sm text-muted-foreground">
        The {this.props.what} failed. Reload the page to try again.
      </p>
    ) : (
      this.props.children
    );
  }
}
