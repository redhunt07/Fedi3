import 'dart:async';
import 'package:material_ui/material_ui.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

class PairingScanner extends StatefulWidget {
  const PairingScanner(
      {super.key, required this.enabled, required this.onCode});
  final bool enabled;
  final ValueChanged<String> onCode;
  @override
  State<PairingScanner> createState() => _PairingScannerState();
}

class _PairingScannerState extends State<PairingScanner> {
  final _controller =
      MobileScannerController(formats: const [BarcodeFormat.qrCode]);
  @override
  void didUpdateWidget(covariant PairingScanner oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.enabled != oldWidget.enabled) {
      unawaited(widget.enabled ? _controller.start() : _controller.stop());
    }
  }

  @override
  void dispose() {
    unawaited(_controller.dispose());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MobileScanner(
        controller: _controller,
        onDetect: (capture) {
          if (!widget.enabled) return;
          for (final barcode in capture.barcodes) {
            final raw = barcode.rawValue;
            if (raw != null && raw.isNotEmpty) {
              widget.onCode(raw);
              break;
            }
          }
        },
      );
}
