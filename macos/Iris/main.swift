import Foundation

// Claude Code starts this binary as the assistant's MCP server, with
// `--mcp-bridge <socket>`. That copy only relays tool calls to the window
// already running, so it answers before SwiftUI opens anything.
let arguments = CommandLine.arguments
if arguments.count > 2, arguments[1] == "--mcp-bridge" {
    exit(runMcpBridge(socket: arguments[2]))
}
IrisApp.main()
