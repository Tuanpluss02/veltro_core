import 'package:veltro/veltro.dart';

part 'state.g.dart';

enum ConnectivityStatus { connected, disconnected }

@Veltro(json: false)
abstract class AppState with _$AppState {
  const factory AppState({
    @Default(false) bool isLoading,
    @Default(ConnectivityStatus.disconnected) ConnectivityStatus connectivity,
    String? error,
  }) = _AppState;
}
