import AppKit
import UniformTypeIdentifiers

/// Files dragged onto the composer, read off the drag: files from Finder or
/// the desktop, files an app promises rather than hands over (Photos, Mail
/// and most apps that drag something they hold), and pictures dragged as
/// data. Each arrives as a file on disk, so the composer attaches them the
/// way it attaches files it is given any other way.
@MainActor
enum FileDrop {
    /// What a view takes in a drag for this.
    static var types: [NSPasteboard.PasteboardType] {
        [.fileURL, .png, .tiff] + NSFilePromiseReceiver.readableDraggedTypes.map { NSPasteboard.PasteboardType($0) }
    }

    private static let filesOnly: [NSPasteboard.ReadingOptionKey: Any] = [.urlReadingFileURLsOnly: true]

    /// Whether the drag carries something to attach, rather than text.
    static func carriesFiles(_ pasteboard: NSPasteboard) -> Bool {
        pasteboard.canReadObject(forClasses: [NSURL.self], options: filesOnly)
            || pasteboard.canReadObject(forClasses: [NSFilePromiseReceiver.self], options: nil)
            || pasteboard.availableType(from: [.png, .tiff]) != nil
    }

    /// Reads what the drag carries and hands over the files, with how many
    /// could not be had. A promised file is written by the app that made the
    /// promise, so this answers once every one has arrived or failed.
    static func read(_ pasteboard: NSPasteboard, done: @escaping @MainActor ([URL], Int) -> Void) {
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: filesOnly) as? [URL], !urls.isEmpty {
            done(urls, 0)
            return
        }
        if let promises = pasteboard.readObjects(forClasses: [NSFilePromiseReceiver.self], options: nil) as? [NSFilePromiseReceiver],
           !promises.isEmpty {
            receive(promises, done: done)
            return
        }
        if let data = pasteboard.data(forType: .png) ?? pasteboard.data(forType: .tiff).flatMap(png) {
            let url = image(data, named: tr("Image"))
            done(url.map { [$0] } ?? [], url == nil ? 1 : 0)
            return
        }
        done([], 0)
    }

    private static func receive(_ promises: [NSFilePromiseReceiver], done: @escaping @MainActor ([URL], Int) -> Void) {
        let destination: URL
        do {
            destination = try folder()
        } catch {
            done([], promises.count)
            return
        }
        let gathering = Gathering(expected: promises.reduce(0) { $0 + max($1.fileNames.count, 1) }, done: done)
        let queue = OperationQueue()
        queue.qualityOfService = .userInitiated
        for promise in promises {
            promise.receivePromisedFiles(atDestination: destination, options: [:], operationQueue: queue) { url, error in
                gathering.arrived(error == nil ? url : nil)
            }
        }
    }

    /// Writes a picture dragged as PNG data to a file called `name`, for it
    /// to be attached as files are. `name` is translated, since the person
    /// writing and the person reading both see it.
    nonisolated static func image(_ png: Data, named name: String) -> URL? {
        guard let url = try? folder().appendingPathComponent(name + ".png"), (try? png.write(to: url)) != nil else {
            return nil
        }
        return url
    }

    /// A folder of its own for one drop, so two files of one name both land.
    nonisolated private static func folder() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("Iris Drops", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private static func png(_ tiff: Data) -> Data? {
        NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:])
    }
}

/// Counts promised files in as the apps write them, on their queue, and
/// answers on the main thread once all are in.
private final class Gathering: @unchecked Sendable {
    private let lock = NSLock()
    private var left: Int
    private var urls: [URL] = []
    private var failed = 0
    private let done: @MainActor ([URL], Int) -> Void

    init(expected: Int, done: @escaping @MainActor ([URL], Int) -> Void) {
        left = expected
        self.done = done
    }

    func arrived(_ url: URL?) {
        lock.lock()
        if let url { urls.append(url) } else { failed += 1 }
        left -= 1
        let finished = left == 0
        let (urls, failed, done) = (self.urls, self.failed, self.done)
        lock.unlock()
        guard finished else { return }
        DispatchQueue.main.async { MainActor.assumeIsolated { done(urls, failed) } }
    }
}
