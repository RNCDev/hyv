import Foundation
import Combine
import AppKit
import os

private let logger = Logger(subsystem: "com.hyv.app", category: "meeting-detection")

@MainActor
final class MeetingDetectorService: ObservableObject {
    @Published var detectedApp: MeetingApp? = nil
    @Published var isMeetingActive: Bool = false

    /// Any meeting app running (including background apps), used for transcript labeling
    var runningMeetingApp: MeetingApp? {
        let running = ProcessUtils.runningBundleIdentifiers()
        return MeetingApp.allCases.first { running.contains($0.rawValue) }
    }

    private var timerCancellable: AnyCancellable?

    func start() {
        logger.info("Meeting detection started (polling every 3s)")
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

        // Only auto-detect apps that don't run persistently in the background
        for app in MeetingApp.allCases where !app.runsInBackground {
            if running.contains(app.rawValue) {
                if detectedApp != app {
                    logger.info("Meeting app detected: \(app.displayName)")
                }
                detectedApp = app
                isMeetingActive = true
                return
            }
        }

        if detectedApp != nil {
            logger.info("Meeting app no longer running: \(self.detectedApp!.displayName)")
        }
        detectedApp = nil
        isMeetingActive = false
    }
}
