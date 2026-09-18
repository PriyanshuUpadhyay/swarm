import Foundation

/// How long is left, and at what time it ends.
///
/// **What used to be here was `UsageFormat`**, which printed every figure the usage panel showed:
/// percentages, money, counts, reset countdowns, deadlines. Swarm no longer reports usage, because
/// Jellow already does, and two of its functions turned out to be about neither usage nor money.
/// Keep Awake uses both to say "2h 14m left, until 4:30 pm", so they are kept under a name that
/// says what they are.
///
/// `Locale`, the calendar and the clock style are parameters, so a test can pin every one of them.
public enum ClockFormat {
    /// "18d 23h", "4h 57m", "5h", "12m". Never seconds, and never zero minutes: a window that
    /// ends in forty seconds ends in "1m".
    public static func compactDuration(_ seconds: TimeInterval) -> String {
        let totalMinutes = max(1, Int((seconds / 60).rounded(.up)))
        let days = totalMinutes / (24 * 60)
        let hours = (totalMinutes % (24 * 60)) / 60
        let minutes = totalMinutes % 60
        if days > 0 { return "\(days)d \(hours)h" }
        if hours > 0 { return minutes > 0 ? "\(hours)h \(minutes)m" : "\(hours)h" }
        return "\(minutes)m"
    }

    /// Fixed patterns for the two forced clocks rather than a format style's hour field, because a
    /// style's `hour` still follows the locale's own cycle: an American locale asked for the 24
    /// hour clock came back as "5:30", meaning half past five in the afternoon.
    public static func timeOfDay(
        _ date: Date,
        clock: ClockStyle,
        calendar: Calendar,
        locale: Locale
    ) -> String {
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        switch clock {
        case .automatic:
            formatter.dateStyle = .none
            formatter.timeStyle = .short
        case .twelveHour:
            formatter.dateFormat = "h:mm a"
        case .twentyFourHour:
            formatter.dateFormat = "HH:mm"
        }
        return formatter.string(from: date)
    }
}

/// The clock an exact time is printed on.
public enum ClockStyle: String, CaseIterable, Sendable {
    case automatic = "auto"
    case twelveHour = "12h"
    case twentyFourHour = "24h"

    public var title: String {
        switch self {
        case .automatic: "Auto"
        case .twelveHour: "12-hour"
        case .twentyFourHour: "24-hour"
        }
    }
}
