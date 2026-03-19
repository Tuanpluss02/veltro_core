import 'package:veltro/veltro.dart';

part 'simple.g.dart';

@Data()
abstract class User with _$User {
  const factory User({
    required String id,
    required String name,
    required int age,
  }) = _User;

  factory User.fromJson(Map<String, dynamic> json) => _$UserFromJson(json);
}
