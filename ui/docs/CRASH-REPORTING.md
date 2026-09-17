# Crash reporting

Bloom uses [spatie/flare-client-swift](https://github.com/spatie/flare-client-swift) through Swift Package Manager. Both `Flare` and `FlareCrashReporter` belong to the app target; BloomCore keeps the build eligibility policy and has no dependency on the SDK.

Capture runs in release and master builds with the `be.spatie.bloom` bundle identifier, unless a debugger is attached or **Send crash reports** is off in General settings. Preference changes take effect after restarting. Local, dev and unbundled builds do not report automatically. Releases use Flare's production environment, master builds use development, and the explicit probe uses testing.

PLCrashReporter saves a native report at the crash. After the next launch, an independent task uploads pending reports. Neither launch nor quit waits for that task. Upload failures are logged and remain queued for another launch. The SDK bounds the queue, uses request timeouts and returns upload failures without throwing. The embedded key is the Bloom App project's public ingestion key, not an account access token.

Reports contain build details, macOS version, Mac model, total RAM, binary identities and stack frames. Flare's Application context shows memory in readable units such as `48.00 GB` and `3.52 MB`, with the sample time alongside it. Available memory is a timestamped estimate of free plus inactive system memory. The SDK refreshes that snapshot every 30 seconds while the app runs and preserves the pre-crash sample when sending after restart. Bloom does not add transcripts, prompts, repository contents or user identity. Native source-level symbolication is separate; binary names and offsets are retained for it. The SDK currently uses JavaScript compatibility metadata, so Flare labels the reports as JavaScript.

The package's own test suite additionally covers HTTP rejection, offline requests, malformed reports, concurrent calls, size limits and native conversion. Run it against the resolved dependency if changing the integration:

```shell
swift test --package-path .build/checkouts/flare-client-swift --jobs 4
```
