import SwiftUI

@main
struct HyvApp: App {
    @StateObject private var appState = AppState()

    var body: some Scene {
        MenuBarExtra("Hyv", systemImage: appState.menuBarIcon) {
            MenuBarView()
                .environmentObject(appState)
        }
        .menuBarExtraStyle(.window)
    }
}
