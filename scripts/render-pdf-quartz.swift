// Development QA: fixed 200 DPI, no antialiasing/interpolation, visible CropBox.
import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers
let args = CommandLine.arguments
if args.count != 4 { fatalError("input output page") }
guard let doc = CGPDFDocument(URL(fileURLWithPath: args[1]) as CFURL), let page = doc.page(at: Int(args[3])!) else { fatalError("cannot read PDF page") }
let box = page.getBoxRect(.cropBox)
var unit: CGPDFReal = 1
if let dictionary = page.dictionary {
    var supplied: CGPDFReal = 1
    if CGPDFDictionaryGetNumber(dictionary, "UserUnit", &supplied), supplied > 0 { unit = supplied }
}
let rotated = abs(page.rotationAngle) % 180 != 0
let scale: CGFloat = 200 / 72 * CGFloat(unit)
let width = Int(ceil((rotated ? box.height : box.width) * scale))
let height = Int(ceil((rotated ? box.width : box.height) * scale))
guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { fatalError("context") }
context.setFillColor(CGColor(gray: 1, alpha: 1)); context.fill(CGRect(x: 0, y: 0, width: width, height: height))
context.interpolationQuality = .none; context.setShouldAntialias(false)
context.concatenate(page.getDrawingTransform(.cropBox, rect: CGRect(x: 0, y: 0, width: width, height: height), rotate: 0, preserveAspectRatio: true)); context.drawPDFPage(page)
guard let image = context.makeImage(), let destination = CGImageDestinationCreateWithURL(URL(fileURLWithPath: args[2]) as CFURL, UTType.png.identifier as CFString, 1, nil) else { fatalError("image") }
CGImageDestinationAddImage(destination, image, nil)
if !CGImageDestinationFinalize(destination) { fatalError("write") }
