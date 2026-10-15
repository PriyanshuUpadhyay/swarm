import Foundation

/// The time-of-day theme. Fixed clock hours, so the HUD needs no location permission.
public enum DayPhase: String, CaseIterable, Sendable {
    case dawn, day, dusk, night

    /// dawn 05-09, day 09-17, dusk 17-21, night 21-05, in local time.
    public static func at(hour: Int) -> DayPhase {
        switch hour {
        case 5..<9: return .dawn
        case 9..<17: return .day
        case 17..<21: return .dusk
        default: return .night
        }
    }

    public static func at(_ date: Date, calendar: Calendar = .current) -> DayPhase {
        at(hour: calendar.component(.hour, from: date))
    }

    /// The next moment the phase changes, so the HUD re-themes without polling the clock.
    public static func nextChange(after date: Date, calendar: Calendar = .current) -> Date {
        let hour = calendar.component(.hour, from: date)
        let next = [5, 9, 17, 21].first { $0 > hour } ?? 5
        let day = next > hour ? date : calendar.date(byAdding: .day, value: 1, to: date)!
        return calendar.date(bySettingHour: next, minute: 0, second: 0, of: day)!
    }

    public var title: String { rawValue.capitalized }
}
