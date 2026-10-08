import Foundation

public enum CountText {
    public static func count(_ value: Int, singular: String, plural: String) -> String {
        "\(value) \(value == 1 ? singular : plural)"
    }

    public static func agentsStillRunning(_ value: Int) -> String {
        count(value, singular: "agent", plural: "agents") + (value == 1 ? " still runs" : " still run")
    }
}
