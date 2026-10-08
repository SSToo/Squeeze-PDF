// Development-only CoreGraphics PDF page rendering for compatibility QA.
// swiftc scripts/render-quartz.swift -o tmp/render-quartz
import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

let args = CommandLine.arguments
guard args.count == 4, let number = Int(args[3]), number > 0 else {
    fatalError("input.pdf output.png page-number")
}
guard let doc = CGPDFDocument(URL(fileURLWithPath: args[1]) as CFURL),
      let page = doc.page(at: number) else {
    fatalError("cannot read PDF page")
}
let box = page.getBoxRect(.cropBox)
let scale: CGFloat = 200 / 72
let width = Int(ceil(box.width * scale))
let height = Int(ceil(box.height * scale))
guard width > 0, height > 0, width <= 16000, height <= 16000,
      let context = CGContext(data: nil, width: width, height: height,
                              bitsPerComponent: 8, bytesPerRow: width * 4,
                              space: CGColorSpaceCreateDeviceRGB(),
                              bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else {
    fatalError("invalid page dimensions or bitmap context")
}
let rect = CGRect(x: 0, y: 0, width: width, height: height)
context.setFillColor(CGColor(gray: 1, alpha: 1))
context.fill(rect)
// Fixed settings permit exact baseline-versus-candidate comparisons within
// CoreGraphics. They do not imply pixel equivalence with other PDF readers.
context.interpolationQuality = .none
context.setShouldAntialias(false)
// Set the sampling scale explicitly; the page-fitting transform alone can
// leave a small page at its native point size inside a larger pixel canvas.
context.scaleBy(x: scale, y: scale)
let drawingRect = CGRect(x: 0, y: 0, width: CGFloat(width) / scale,
                         height: CGFloat(height) / scale)
context.concatenate(page.getDrawingTransform(.cropBox, rect: drawingRect,
                                            rotate: 0, preserveAspectRatio: true))
context.drawPDFPage(page)
guard let image = context.makeImage(),
      let destination = CGImageDestinationCreateWithURL(
        URL(fileURLWithPath: args[2]) as CFURL, UTType.png.identifier as CFString, 1, nil
      ) else {
    fatalError("cannot create output image")
}
CGImageDestinationAddImage(destination, image, nil)
guard CGImageDestinationFinalize(destination) else { fatalError("cannot write image") }
