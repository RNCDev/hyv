import Foundation
import Combine
import AppKit

@MainActor
final class MeetingDetectorService: ObservableObject {
    @Published var detectedApp: MeetingApp? = nil
    @Published var isMeetingActive: Bool = false

    private var timerCancellable: AnyCancellable?

    func start() {
        timerCancellable = Timer.publish(every: 3, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                self?.checkForMeetings()
            }
        // Also check immediately
        checkForMeetings()
    }

    func stop() {
        timerCancellable?.cancel()
        timerCancellable = nil
        detectedApp = nil
        isMeetingActive = false
    }

    private func checkForMeetings() {
        let running = ProcessUtils.runningBundleIdentifiers()

        for app in MeetingApp.allCases {
            if running.contains(app.rawValue) {
                detectedApp = app
                isMeetingActive = true
                return
            }
        }

        detectedApp = nil
        isMeetingActive = false
    }
}
