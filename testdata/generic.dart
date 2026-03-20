import 'package:veltro/veltro.dart';

part 'generic.g.dart';

@Veltro()
abstract class ApiResponse<T> with _$ApiResponse<T> {
  const factory ApiResponse({required bool success, required T data}) =
      _ApiResponse;
}
