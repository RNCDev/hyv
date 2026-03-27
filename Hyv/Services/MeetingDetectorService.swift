import Foundation
import Combine
import AppKit

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
    private var activateObserver: NSObjectProtocol?
    private var terminateObserver: NSObjectProtocol?

    func start() {
        let center = NSWorkspace.shared.notificationCenter

        activateObserver = center.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.checkForMeetings()
            }
        }

        terminateObserver = center.addObserver(
            forName: NSWorkspace.didTerminateApplicationNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.checkForMeetings()
            }
        }

        // Safety-net poll every 30 seconds
        timerCancellable = Timer.publish(every: AppConstants.meetingDetectionPollInterval, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                self?.checkForMeetings()
            }

        // Also check immediately
        checkForMeetings()
    }

    func stop() {
        let center = NSWorkspace.shared.notificationCenter
        if let observer = activateObserver {
            center.removeObserver(observer)
            activateObserver = nil
        }
        if let observer = terminateObserver {
            center.removeObserver(observer)
            terminateObserver = nil
        }
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
                detectedApp = app
                isMeetingActive = true
                return
            }
        }

        detectedApp = nil
        isMeetingActive = false
    }
}
