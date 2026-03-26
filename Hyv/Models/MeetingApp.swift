enum MeetingApp: String, CaseIterable {
    case zoom = "us.zoom.xos"
    case teams = "com.microsoft.teams2"
    case teamsClassic = "com.microsoft.teams"
    case facetime = "com.apple.FaceTime"
    case webex = "com.webex.meetingmanager"
    case slack = "com.tinyspeck.slackmacgap"

    var displayName: String {
        switch self {
        case .zoom: return "Zoom"
        case .teams, .teamsClassic: return "Microsoft Teams"
        case .facetime: return "FaceTime"
        case .webex: return "Webex"
        case .slack: return "Slack"
        }
    }
}
