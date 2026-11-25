import CoreImage.CIFilterBuiltins
import ImageIO
import UIKit
import UniformTypeIdentifiers

/// A QR code of what the core made for it, drawn by Core Image, dark on white, a pixel a
/// module, for the view to scale up without smoothing.
enum QRPicture {
    static func of(_ text: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let image = filter.outputImage,
              let cg = CIContext().createCGImage(image, from: image.extent) else { return nil }
        return UIImage(cgImage: cg)
    }
}

/// Pictures read, decoded and made smaller off the main thread, and kept once they are,
/// so a row that's built again doesn't decode its picture again.
enum Pictures {
    /// A picture already decoded, if it is.
    static func kept(_ path: String, side: CGFloat? = nil) -> UIImage? {
        keptImages.object(forKey: key(path, side))
    }

    /// The picture at `path`, decoded off the main thread, no bigger than `side` points
    /// across when that's given. Nil if it can't be read.
    static func load(_ path: String, side: CGFloat? = nil) async -> UIImage? {
        if let image = kept(path, side: side) { return image }
        let image = await Task.detached(priority: .userInitiated) { () -> UIImage? in
            if let side {
                // At 3× for the sharpest screen.
                return smaller(path, pixels: side * 3)
            }
            return UIImage(contentsOfFile: path)?.preparingForDisplay()
        }.value
        if let image {
            let pixels = Int(image.size.width * image.scale * image.size.height * image.scale)
            keptImages.setObject(image, forKey: key(path, side), cost: pixels * 4)
        }
        return image
    }

    /// A picture's size in pixels, turned the way it shows, from its file's header alone,
    /// kept by path: a file's path is its SHA-256, so what's there doesn't change shape.
    /// Nil if the header can't be read.
    @MainActor
    static func size(_ path: String) -> CGSize? {
        if let kept = sizes[path] { return kept }
        var out: CGSize?
        if let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil),
           let p = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
           let w = p[kCGImagePropertyPixelWidth] as? Int, let h = p[kCGImagePropertyPixelHeight] as? Int {
            // EXIF orientations 5 to 8 turn it a quarter.
            let turned = ((p[kCGImagePropertyOrientation] as? Int) ?? 1) >= 5
            out = turned ? CGSize(width: h, height: w) : CGSize(width: w, height: h)
        }
        sizes[path] = .some(out)
        return out
    }

    @MainActor private static var sizes: [String: CGSize?] = [:]

    /// The size to draw a picture of `size` at, at most `most`: scaled down to fit both,
    /// its shape kept, and never up, as the desktop's `fit`.
    static func fit(_ size: CGSize, in most: CGSize) -> CGSize {
        let w = max(size.width, 1), h = max(size.height, 1)
        let k = min(most.width / w, most.height / h, 1)
        return CGSize(width: max((w * k).rounded(), 1), height: max((h * k).rounded(), 1))
    }

    /// A photo's data as a JPEG at `url`, turned the way it was taken and without its
    /// location or other metadata. Run it off the main thread: a 12 MP photo takes tens
    /// of milliseconds.
    static func jpeg(_ data: Data, to url: URL) -> Bool {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int
        else { return false }
        // Full size, with its turn put into the pixels.
        let whole: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: max(width, height),
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, whole as CFDictionary),
              let out = CGImageDestinationCreateWithURL(url as CFURL, UTType.jpeg.identifier as CFString, 1, nil)
        else { return false }
        CGImageDestinationAddImage(out, image, [kCGImageDestinationLossyCompressionQuality: 0.85] as CFDictionary)
        return CGImageDestinationFinalize(out)
    }

    private static func smaller(_ path: String, pixels: CGFloat) -> UIImage? {
        guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: max(Int(pixels), 1),
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        return UIImage(cgImage: image)
    }

    private static func key(_ path: String, _ side: CGFloat?) -> NSString {
        side.map { "\(path)#\(Int($0))" as NSString } ?? path as NSString
    }

    nonisolated(unsafe) private static let keptImages: NSCache<NSString, UIImage> = {
        let c = NSCache<NSString, UIImage>()
        c.totalCostLimit = 64 << 20
        return c
    }()
}
