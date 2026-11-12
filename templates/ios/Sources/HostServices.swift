import AVFoundation
import Foundation
import Security
import UIKit

// NEVER COMPILED. This file was written on a machine without Xcode, against
// the C interface in morningstar_ios.h and from the documentation of
// URLSession, AVAudioRecorder, AVAudioPlayer and UserDefaults. It has been
// parsed (swiftc -parse) and nothing more: no type has been checked and no
// line has run. Expect to fix it the first time it is built. See
// docs/architecture/0019-effects-and-platform-services.md, "Not verified".

/// The platform services an application can ask for: HTTP requests,
/// recording from the microphone, playing a clip, and a few stored values.
///
/// `HostViewController` owns one. After every call into the engine it calls
/// `drain(_:)`, which takes the application's requests and starts the work;
/// each answer comes back through `owner.deliverServiceResult`, on the main
/// thread, and only while the controller still has its engine. Everything
/// in this class runs on the main thread: URLSession does its networking on
/// its own threads and is told to call its delegate on the main queue, and
/// the audio classes do their work on theirs. The one piece of work done on
/// another queue here is reading the finished recording from disk.
final class HostServices: NSObject, URLSessionDataDelegate, AVAudioRecorderDelegate,
    AVAudioPlayerDelegate
{
    /// Must match MAX_HTTP_RESPONSE_BYTES in morningstar-core.
    private static let maxResponseBytes = 10 * 1024 * 1024
    /// Must match MAX_RECORDING_DURATION in morningstar-core.
    private static let maxRecordingSeconds: TimeInterval = 60

    private static let installIDKey = "dev.morningstar.install_id"
    /// One dictionary of String to String holds the application's values.
    private static let storeKey = "dev.morningstar.store"

    /// The controller whose engine the answers go to. Weak: when it is
    /// gone, answers are dropped, which is what must happen to them.
    weak var owner: HostViewController?

    // MARK: - What the application reads at setup

    /// This installation's identifier: 128 bits from the system's secure
    /// random generator as 32 hexadecimal digits, made on first use and kept
    /// in UserDefaults. It goes when the app is deleted.
    static func installID() -> String {
        let defaults = UserDefaults.standard
        if let id = defaults.string(forKey: installIDKey), id.count == 32 {
            return id
        }
        var random = [UInt8](repeating: 0, count: 16)
        if SecRandomCopyBytes(kSecRandomDefault, random.count, &random) != errSecSuccess {
            // Not expected to fail; UUIDs come from the same generator.
            let uuid = UUID().uuid
            random = [
                uuid.0, uuid.1, uuid.2, uuid.3, uuid.4, uuid.5, uuid.6, uuid.7,
                uuid.8, uuid.9, uuid.10, uuid.11, uuid.12, uuid.13, uuid.14, uuid.15,
            ]
        }
        let id = random.map { String(format: "%02x", $0) }.joined()
        defaults.set(id, forKey: installIDKey)
        return id
    }

    /// Everything the application has stored, as keys and values in turn.
    static func storedSnapshot() -> [String] {
        var flat: [String] = []
        for (key, value) in storedValues() {
            flat.append(key)
            flat.append(value)
        }
        return flat
    }

    private static func storedValues() -> [String: String] {
        (UserDefaults.standard.dictionary(forKey: storeKey) as? [String: String]) ?? [:]
    }

    // MARK: - Lifecycle

    private lazy var session: URLSession = {
        // No cookies, no cache, nothing written to disk: each request is
        // what the application said and no more.
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        return URLSession(configuration: configuration, delegate: self, delegateQueue: .main)
    }()

    override init() {
        super.init()
        let center = NotificationCenter.default
        // Sound stops when the app leaves the screen: a clip ends as
        // interrupted and a recording is closed and kept.
        center.addObserver(
            self, selector: #selector(quiet),
            name: UIApplication.didEnterBackgroundNotification, object: nil)
        // A phone call or another app taking the audio session.
        center.addObserver(
            self, selector: #selector(interrupted(_:)),
            name: AVAudioSession.interruptionNotification, object: nil)
    }

    /// The controller is going away: stop everything and let go of the
    /// session, which otherwise keeps its delegate (this object) alive.
    func shutDown() {
        NotificationCenter.default.removeObserver(self)
        owner = nil
        exchanges.removeAll()
        session.invalidateAndCancel()
        player?.stop()
        player = nil
        recorder?.stop()
        recorder = nil
        removeRecordingFile()
    }

    @objc private func quiet() {
        endPlayback(error: MORNINGSTAR_AUDIO_INTERRUPTED, message: nil)
        finishRecording()
    }

    @objc private func interrupted(_ notification: Notification) {
        guard
            let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
            AVAudioSession.InterruptionType(rawValue: raw) == .began
        else { return }
        quiet()
    }

    // MARK: - Requests

    /// Takes every request the application has made and starts the work.
    func drain(_ engine: OpaquePointer) {
        var request = MorningstarServiceRequest()
        while morningstar_ios_take_service_request(engine, &request) {
            // The request's memory is only valid until the next take, so
            // everything is copied here, before the loop goes round.
            let id = request.id
            let text1 = String(cString: request.text1)
            let text2 = String(cString: request.text2)
            let text3 = String(cString: request.text3)
            let data: Data
            if let bytes = request.data, request.data_len > 0 {
                data = Data(bytes: bytes, count: request.data_len)
            } else {
                data = Data()
            }
            switch Int(request.kind) {
            case MORNINGSTAR_SERVICE_HTTP:
                startHTTP(
                    id: id, method: text1, url: text2, headers: text3, body: data,
                    timeout: TimeInterval(request.timeout_ms) / 1000)
            case MORNINGSTAR_SERVICE_START_RECORDING:
                startRecording(id: id)
            case MORNINGSTAR_SERVICE_STOP_RECORDING:
                stopRecording(id: id)
            case MORNINGSTAR_SERVICE_PLAY:
                play(id: id, mime: text1, data: data)
            case MORNINGSTAR_SERVICE_STOP_PLAYBACK:
                endPlayback(error: MORNINGSTAR_AUDIO_INTERRUPTED, message: nil)
            case MORNINGSTAR_SERVICE_STORE:
                var values = HostServices.storedValues()
                values[text1] = text2
                UserDefaults.standard.set(values, forKey: HostServices.storeKey)
            case MORNINGSTAR_SERVICE_REMOVE:
                var values = HostServices.storedValues()
                values[text1] = nil
                UserDefaults.standard.set(values, forKey: HostServices.storeKey)
            case MORNINGSTAR_SERVICE_LOAD:
                let value = HostServices.storedValues()[text1]
                answer { engine in
                    if let value = value {
                        value.withCString { morningstar_ios_load_result(engine, id, true, $0) }
                    } else {
                        morningstar_ios_load_result(engine, id, false, nil)
                    }
                }
            default:
                // A library newer than this host. Nothing here knows whether
                // the request expects an answer, so none can be given.
                NSLog("%@", "morningstar: unknown service request kind \(request.kind)")
            }
        }
    }

    /// Hands an answer to the engine: on the main thread, on a later turn
    /// of the run loop (never inside `drain`, which is itself inside a call
    /// into the engine), and only if the controller and its engine are
    /// still there.
    private func answer(_ call: @escaping (OpaquePointer) -> Void) {
        DispatchQueue.main.async { [weak self] in
            self?.owner?.deliverServiceResult(call)
        }
    }

    private func answerAudio(id: UInt64, kind: Int, error: Int, message: String?) {
        answer { engine in
            if let message = message {
                message.withCString {
                    morningstar_ios_audio_result(
                        engine, id, UInt32(kind), UInt32(error), $0, nil, 0, nil, 0)
                }
            } else {
                morningstar_ios_audio_result(
                    engine, id, UInt32(kind), UInt32(error), nil, nil, 0, nil, 0)
            }
        }
    }

    // MARK: - HTTP

    /// One request in flight. Main thread only.
    private final class Exchange {
        let id: UInt64
        var body = Data()
        var tooLarge = false
        var timedOut = false

        init(id: UInt64) {
            self.id = id
        }
    }

    /// By `URLSessionTask.taskIdentifier`.
    private var exchanges: [Int: Exchange] = [:]

    private func answerHTTP(id: UInt64, error: Int, message: String?) {
        answer { engine in
            if let message = message {
                message.withCString {
                    morningstar_ios_http_result(engine, id, UInt32(error), 0, nil, nil, 0, $0)
                }
            } else {
                morningstar_ios_http_result(engine, id, UInt32(error), 0, nil, nil, 0, nil)
            }
        }
    }

    private func startHTTP(
        id: UInt64, method: String, url: String, headers: String, body: Data,
        timeout: TimeInterval
    ) {
        guard
            let target = URL(string: url),
            let scheme = target.scheme?.lowercased(),
            scheme == "https" || scheme == "http",
            let host = target.host, !host.isEmpty
        else {
            answerHTTP(id: id, error: MORNINGSTAR_HTTP_INVALID_URL, message: nil)
            return
        }
        // App Transport Security decides about cleartext: only what the
        // Info.plist excepts (localhost, for development) is allowed.
        var request = URLRequest(
            url: target, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: timeout)
        request.httpMethod = method
        for line in headers.split(separator: "\n") {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let name = line[..<colon].trimmingCharacters(in: .whitespaces)
            let value = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            if !name.isEmpty {
                request.addValue(value, forHTTPHeaderField: name)
            }
        }
        // As on Android: a body goes only with the methods that carry one.
        let carriesBody = method == "POST" || method == "PUT" || method == "PATCH"
        if carriesBody || (method == "DELETE" && !body.isEmpty) {
            request.httpBody = body
        }

        let task = session.dataTask(with: request)
        exchanges[task.taskIdentifier] = Exchange(id: id)
        // `timeoutInterval` is about silence between packets. The
        // application's timeout is for the whole exchange.
        DispatchQueue.main.asyncAfter(deadline: .now() + timeout) { [weak self, weak task] in
            guard let self = self, let task = task,
                let exchange = self.exchanges[task.taskIdentifier], exchange.id == id
            else { return }
            exchange.timedOut = true
            task.cancel()
        }
        task.resume()
    }

    func urlSession(
        _ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse,
        completionHandler: @escaping (URLSession.ResponseDisposition) -> Void
    ) {
        if response.expectedContentLength > Int64(HostServices.maxResponseBytes) {
            exchanges[dataTask.taskIdentifier]?.tooLarge = true
            completionHandler(.cancel)
        } else {
            completionHandler(.allow)
        }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        guard let exchange = exchanges[dataTask.taskIdentifier] else { return }
        if exchange.body.count + data.count > HostServices.maxResponseBytes {
            exchange.tooLarge = true
            exchange.body = Data()
            dataTask.cancel()
        } else {
            exchange.body.append(data)
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?)
    {
        guard let exchange = exchanges.removeValue(forKey: task.taskIdentifier) else { return }
        let id = exchange.id
        if exchange.tooLarge {
            answerHTTP(id: id, error: MORNINGSTAR_HTTP_TOO_LARGE, message: nil)
            return
        }
        if exchange.timedOut {
            answerHTTP(id: id, error: MORNINGSTAR_HTTP_TIMEOUT, message: nil)
            return
        }
        if let error = error {
            let code = (error as NSError).domain == NSURLErrorDomain ? (error as NSError).code : 0
            switch code {
            case NSURLErrorTimedOut:
                answerHTTP(id: id, error: MORNINGSTAR_HTTP_TIMEOUT, message: nil)
            case NSURLErrorNotConnectedToInternet, NSURLErrorCannotFindHost,
                NSURLErrorCannotConnectToHost, NSURLErrorNetworkConnectionLost,
                NSURLErrorDNSLookupFailed, NSURLErrorInternationalRoamingOff,
                NSURLErrorDataNotAllowed:
                answerHTTP(id: id, error: MORNINGSTAR_HTTP_OFFLINE, message: nil)
            case NSURLErrorBadURL, NSURLErrorUnsupportedURL:
                answerHTTP(id: id, error: MORNINGSTAR_HTTP_INVALID_URL, message: nil)
            default:
                // Includes TLS failures and a cleartext request that App
                // Transport Security refused.
                answerHTTP(
                    id: id, error: MORNINGSTAR_HTTP_FAILED, message: error.localizedDescription)
            }
            return
        }
        guard let response = task.response as? HTTPURLResponse else {
            answerHTTP(id: id, error: MORNINGSTAR_HTTP_FAILED, message: "not an HTTP response")
            return
        }
        var block = ""
        for (name, value) in response.allHeaderFields {
            let line = "\(name): \(value)"
            block += line.replacingOccurrences(of: "\n", with: " ") + "\n"
        }
        let status = UInt32(max(0, response.statusCode))
        let body = exchange.body
        answer { engine in
            block.withCString { headers in
                body.withUnsafeBytes { (bytes: UnsafeRawBufferPointer) in
                    morningstar_ios_http_result(
                        engine, id, UInt32(MORNINGSTAR_HTTP_OK), status, headers,
                        bytes.bindMemory(to: UInt8.self).baseAddress, body.count, nil)
                }
            }
        }
    }

    // MARK: - Recording

    private var recorder: AVAudioRecorder?
    /// Length of a recording that has been closed and not yet asked for.
    private var finishedSeconds: TimeInterval?
    /// A start is waiting for the user's answer about the microphone.
    private var askingForMicrophone = false

    private var recordingURL: URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("morningstar-recording.m4a")
    }

    private func removeRecordingFile() {
        try? FileManager.default.removeItem(at: recordingURL)
    }

    private func startRecording(id: UInt64) {
        if (recorder != nil && finishedSeconds == nil) || askingForMicrophone {
            answerAudio(
                id: id, kind: MORNINGSTAR_AUDIO_KIND_STARTED, error: MORNINGSTAR_AUDIO_BUSY,
                message: nil)
            return
        }
        let audio = AVAudioSession.sharedInstance()
        switch audio.recordPermission {
        case .granted:
            beginRecording(id: id)
        case .denied:
            // iOS asks once. After a refusal only Settings can change it.
            answerAudio(
                id: id, kind: MORNINGSTAR_AUDIO_KIND_STARTED,
                error: MORNINGSTAR_AUDIO_PERMISSION_DENIED, message: nil)
        default:
            askingForMicrophone = true
            audio.requestRecordPermission { [weak self] granted in
                // Called on an arbitrary queue.
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    self.askingForMicrophone = false
                    if granted {
                        self.beginRecording(id: id)
                    } else {
                        self.answerAudio(
                            id: id, kind: MORNINGSTAR_AUDIO_KIND_STARTED,
                            error: MORNINGSTAR_AUDIO_PERMISSION_DENIED, message: nil)
                    }
                }
            }
        }
    }

    private func beginRecording(id: UInt64) {
        // A recording nobody collected is replaced by the new one.
        recorder = nil
        finishedSeconds = nil
        removeRecordingFile()
        do {
            let audio = AVAudioSession.sharedInstance()
            try audio.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker])
            try audio.setActive(true)
            // Speech: 16 kHz mono AAC in an M4A container, as on Android.
            let settings: [String: Any] = [
                AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
                AVSampleRateKey: 16_000,
                AVNumberOfChannelsKey: 1,
                AVEncoderBitRateKey: 32_000,
                AVEncoderAudioQualityKey: AVAudioQuality.medium.rawValue,
            ]
            let created = try AVAudioRecorder(url: recordingURL, settings: settings)
            created.delegate = self
            guard created.record(forDuration: HostServices.maxRecordingSeconds) else {
                throw NSError(
                    domain: "dev.morningstar", code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "the recorder did not start"])
            }
            recorder = created
            answerAudio(
                id: id, kind: MORNINGSTAR_AUDIO_KIND_STARTED, error: MORNINGSTAR_AUDIO_OK,
                message: nil)
        } catch {
            removeRecordingFile()
            answerAudio(
                id: id, kind: MORNINGSTAR_AUDIO_KIND_STARTED, error: MORNINGSTAR_AUDIO_FAILED,
                message: error.localizedDescription)
        }
    }

    /// Closes the running recording, if any, and keeps it for the stop that
    /// will ask for it.
    private func finishRecording() {
        guard let recorder = recorder, finishedSeconds == nil else { return }
        // `currentTime` is zero once the recorder has stopped.
        finishedSeconds = min(recorder.currentTime, HostServices.maxRecordingSeconds)
        recorder.stop()
    }

    // The recorder reached its maximum length by itself.
    func audioRecorderDidFinishRecording(_ recorder: AVAudioRecorder, successfully flag: Bool) {
        if recorder === self.recorder && finishedSeconds == nil {
            finishedSeconds = HostServices.maxRecordingSeconds
        }
    }

    private func stopRecording(id: UInt64) {
        finishRecording()
        guard recorder != nil, let seconds = finishedSeconds else {
            answerAudio(
                id: id, kind: MORNINGSTAR_AUDIO_KIND_RECORDING,
                error: MORNINGSTAR_AUDIO_NOT_RECORDING, message: nil)
            return
        }
        recorder = nil
        finishedSeconds = nil
        let url = recordingURL
        // The file is small, but reading it is still disk work.
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let data = try? Data(contentsOf: url)
            try? FileManager.default.removeItem(at: url)
            DispatchQueue.main.async {
                guard let self = self else { return }
                guard let data = data, !data.isEmpty else {
                    self.answerAudio(
                        id: id, kind: MORNINGSTAR_AUDIO_KIND_RECORDING,
                        error: MORNINGSTAR_AUDIO_FAILED, message: "nothing was recorded")
                    return
                }
                let millis = UInt64(max(0, seconds) * 1000)
                self.answer { engine in
                    "audio/mp4".withCString { mime in
                        data.withUnsafeBytes { (bytes: UnsafeRawBufferPointer) in
                            morningstar_ios_audio_result(
                                engine, id, UInt32(MORNINGSTAR_AUDIO_KIND_RECORDING),
                                UInt32(MORNINGSTAR_AUDIO_OK), nil,
                                bytes.bindMemory(to: UInt8.self).baseAddress, data.count, mime,
                                millis)
                        }
                    }
                }
            }
        }
    }

    // MARK: - Playback

    private var player: AVAudioPlayer?
    /// The request whose clip is playing.
    private var playingID: UInt64 = 0

    private func play(id: UInt64, mime: String, data: Data) {
        // One clip at a time: the one playing ends as interrupted.
        endPlayback(error: MORNINGSTAR_AUDIO_INTERRUPTED, message: nil)
        guard UIApplication.shared.applicationState != .background else {
            // An answer that arrived in the background must not start
            // talking from there.
            answerAudio(
                id: id, kind: MORNINGSTAR_AUDIO_KIND_PLAYED,
                error: MORNINGSTAR_AUDIO_INTERRUPTED, message: nil)
            return
        }
        let type = mime.split(separator: ";").first.map {
            $0.trimmingCharacters(in: .whitespaces).lowercased()
        }
        let hint: String?
        switch type ?? "" {
        case "audio/mpeg", "audio/mp3": hint = AVFileType.mp3.rawValue
        case "audio/mp4", "audio/m4a", "audio/x-m4a", "audio/aac": hint = AVFileType.m4a.rawValue
        case "audio/wav", "audio/x-wav": hint = AVFileType.wav.rawValue
        default: hint = nil
        }
        do {
            let audio = AVAudioSession.sharedInstance()
            // While recording the session is already play-and-record.
            if recorder == nil || finishedSeconds != nil {
                try audio.setCategory(.playback, mode: .spokenAudio)
            }
            try audio.setActive(true)
            let created = try AVAudioPlayer(data: data, fileTypeHint: hint)
            created.delegate = self
            guard created.play() else {
                throw NSError(
                    domain: "dev.morningstar", code: 2,
                    userInfo: [NSLocalizedDescriptionKey: "the clip did not start"])
            }
            player = created
            playingID = id
        } catch {
            answerAudio(
                id: id, kind: MORNINGSTAR_AUDIO_KIND_PLAYED, error: MORNINGSTAR_AUDIO_FAILED,
                message: error.localizedDescription)
        }
    }

    /// Ends the clip that is playing, if any, and answers its request, once.
    private func endPlayback(error: Int, message: String?) {
        guard let ending = player else { return }
        let id = playingID
        player = nil
        playingID = 0
        ending.delegate = nil
        ending.stop()
        if recorder == nil || finishedSeconds != nil {
            // Let other apps' audio come back.
            try? AVAudioSession.sharedInstance().setActive(
                false, options: .notifyOthersOnDeactivation)
        }
        answerAudio(id: id, kind: MORNINGSTAR_AUDIO_KIND_PLAYED, error: error, message: message)
    }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        guard player === self.player else { return }
        if flag {
            endPlayback(error: MORNINGSTAR_AUDIO_OK, message: nil)
        } else {
            endPlayback(error: MORNINGSTAR_AUDIO_FAILED, message: "the clip could not be decoded")
        }
    }

    func audioPlayerDecodeErrorDidOccur(_ player: AVAudioPlayer, error: Error?) {
        guard player === self.player else { return }
        endPlayback(
            error: MORNINGSTAR_AUDIO_FAILED,
            message: error?.localizedDescription ?? "the clip could not be decoded")
    }
}
