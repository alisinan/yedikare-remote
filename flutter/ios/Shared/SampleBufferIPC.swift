import Foundation

/// Shared constants and IPC helpers for the Broadcast Extension ↔ Main App communication.
/// Both the extension and main app use a memory-mapped file in the App Group container
/// to transfer screen frame data efficiently.

let kAppGroupIdentifier = "group.com.hoptodesk.app"
let kSharedFrameFileName = "screen_frame.bin"
let kDarwinNotificationName = "com.hoptodesk.app.newframe"
let kBroadcastStartedNotificationName = "com.hoptodesk.app.broadcaststarted"
let kBroadcastStoppedNotificationName = "com.hoptodesk.app.broadcaststopped"
let kBroadcastStopRequestNotificationName = "com.hoptodesk.app.broadcaststoprequest"
let kBroadcastStopRequestKey = "com.hoptodesk.app.stop_broadcast"
let kBroadcastActiveTimestampKey = "com.hoptodesk.app.broadcast_active_ts"

/// Shared frame buffer layout:
/// [0..4]   magic: UInt32 = 0x48445346 ("HDSF")
/// [4..8]   sequence: UInt32 (incremented each frame)
/// [8..12]  width: UInt32
/// [12..16] height: UInt32
/// [16..20] stride: UInt32 (bytes per row)
/// [20..24] format: UInt32 (0=BGRA)
/// [24..28] data_len: UInt32
/// [28..32] reserved: UInt32
/// [32..]   pixel data (BGRA)
let kFrameHeaderSize: Int = 32
let kFrameMagic: UInt32 = 0x48445346
/// Max buffer: 20MB (enough for 2796×1290×4 ≈ 14.4MB + header)
let kMaxFrameBufferSize: Int = 20 * 1024 * 1024

class SharedFrameWriter {
    private var fileHandle: FileHandle?
    private var mappedData: UnsafeMutableRawPointer?
    private var mappedSize: Int = 0
    private var sequence: UInt32 = 0

    init?() {
        guard let containerURL = FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: kAppGroupIdentifier
        ) else {
            return nil
        }

        let fileURL = containerURL.appendingPathComponent(kSharedFrameFileName)

        // Create file if needed, with max size
        if !FileManager.default.fileExists(atPath: fileURL.path) {
            FileManager.default.createFile(atPath: fileURL.path, contents: nil)
        }

        guard let handle = try? FileHandle(forUpdating: fileURL) else {
            return nil
        }
        self.fileHandle = handle

        // Ensure file is large enough
        try? handle.truncate(atOffset: UInt64(kMaxFrameBufferSize))

        // Memory-map the file
        let fd = handle.fileDescriptor
        let ptr = mmap(nil, kMaxFrameBufferSize, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0)
        guard ptr != MAP_FAILED else {
            return nil
        }
        self.mappedData = ptr
        self.mappedSize = kMaxFrameBufferSize
    }

    func writeFrame(pixelData: UnsafeRawPointer, dataLen: Int, width: UInt32, height: UInt32, stride: UInt32) {
        guard let mapped = mappedData else { return }
        let requiredSize = kFrameHeaderSize + dataLen
        guard requiredSize <= mappedSize else { return }

        sequence += 1

        // Write header
        let headerPtr = mapped.assumingMemoryBound(to: UInt32.self)
        headerPtr[0] = kFrameMagic     // magic
        // sequence written last (acts as memory fence for reader)
        headerPtr[2] = width
        headerPtr[3] = height
        headerPtr[4] = stride
        headerPtr[5] = 0               // format: BGRA
        headerPtr[6] = UInt32(dataLen)
        headerPtr[7] = 0               // reserved

        // Write pixel data
        let dataPtr = mapped.advanced(by: kFrameHeaderSize)
        memcpy(dataPtr, pixelData, dataLen)

        // Write sequence last (signals new frame to reader)
        OSAtomicIncrement32(mapped.assumingMemoryBound(to: Int32.self).advanced(by: 1))
        headerPtr[1] = sequence

        // Post Darwin notification to wake up the main app
        let center = CFNotificationCenterGetDarwinNotifyCenter()
        CFNotificationCenterPostNotification(center, CFNotificationName(kDarwinNotificationName as CFString), nil, nil, true)
    }

    deinit {
        if let mapped = mappedData {
            munmap(mapped, mappedSize)
        }
        fileHandle?.closeFile()
    }
}

class SharedFrameReader {
    private var fileHandle: FileHandle?
    private var mappedData: UnsafeMutableRawPointer?
    private var mappedSize: Int = 0
    private var lastSequence: UInt32 = 0

    init?() {
        guard let containerURL = FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: kAppGroupIdentifier
        ) else {
            return nil
        }

        let fileURL = containerURL.appendingPathComponent(kSharedFrameFileName)
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return nil
        }

        guard let handle = try? FileHandle(forUpdating: fileURL) else {
            return nil
        }
        self.fileHandle = handle

        let fd = handle.fileDescriptor
        let ptr = mmap(nil, kMaxFrameBufferSize, PROT_READ, MAP_SHARED, fd, 0)
        guard ptr != MAP_FAILED else {
            return nil
        }
        self.mappedData = ptr
        self.mappedSize = kMaxFrameBufferSize
    }

    /// Returns (pixelData, width, height, stride) if a new frame is available, nil otherwise.
    func readNewFrame() -> (UnsafeRawPointer, Int, UInt32, UInt32, UInt32)? {
        guard let mapped = mappedData else { return nil }
        let headerPtr = mapped.assumingMemoryBound(to: UInt32.self)

        let magic = headerPtr[0]
        guard magic == kFrameMagic else { return nil }

        let seq = headerPtr[1]
        guard seq != lastSequence else { return nil }
        lastSequence = seq

        let width = headerPtr[2]
        let height = headerPtr[3]
        let stride = headerPtr[4]
        let dataLen = Int(headerPtr[6])

        guard dataLen > 0 && kFrameHeaderSize + dataLen <= mappedSize else { return nil }

        let dataPtr = mapped.advanced(by: kFrameHeaderSize)
        return (UnsafeRawPointer(dataPtr), dataLen, width, height, stride)
    }

    deinit {
        if let mapped = mappedData {
            munmap(mapped, mappedSize)
        }
        fileHandle?.closeFile()
    }
}
