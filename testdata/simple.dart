import 'package:veltro/veltro.dart';

part 'simple.g.dart';

@Veltro()
abstract class User with _$User {
  const factory User({
    required String id,
    required String name,
    required int age,
  }) = _User;
}
