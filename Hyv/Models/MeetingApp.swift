enum MeetingApp: String, CaseIterable {
    case zoom = "us.zoom.xos"
    case teams = "com.microsoft.teams2"
    case teamsClassic = "com.microsoft.teams"
    case facetime = "com.apple.FaceTime"
    case webex = "com.webex.meetingmanager"
    case slack = "com.tinyspeck.slackmacgap"
    case whatsapp = "net.whatsapp.WhatsApp"

    var displayName: String {
        switch self {
        case .zoom: return "Zoom"
        case .teams, .teamsClassic: return "Microsoft Teams"
        case .facetime: return "FaceTime"
        case .webex: return "Webex"
        case .slack: return "Slack"
        case .whatsapp: return "WhatsApp"
        }
    }

    /// Apps that run persistently — don't auto-detect as "meeting active"
    /// They'll still be used to label transcripts when the user manually records
    var runsInBackground: Bool {
        switch self {
        case .teams, .teamsClassic, .slack, .whatsapp:
            return true
        case .zoom, .facetime, .webex:
            return false
        }
    }
}
