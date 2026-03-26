import AppKit

struct ProcessUtils {
    /// Returns the set of currently running application bundle identifiers
    static func runningBundleIdentifiers() -> Set<String> {
        Set(NSWorkspace.shared.runningApplications.compactMap { $0.bundleIdentifier })
    }

    /// Checks if any app with the given bundle identifiers is running
    static func isAnyRunning(bundleIds: [String]) -> String? {
        let running = runningBundleIdentifiers()
        return bundleIds.first { running.contains($0) }
    }
}
