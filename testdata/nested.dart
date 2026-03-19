import 'package:veltro/veltro.dart';

part 'nested.g.dart';

@Data()
abstract class Address with _$Address {
  const factory Address({required String street, required String city}) =
      _Address;

  factory Address.fromJson(Map<String, dynamic> json) =>
      _$AddressFromJson(json);
}

@Data()
abstract class Person with _$Person {
  const factory Person({required String name, required Address address}) =
      _Person;

  factory Person.fromJson(Map<String, dynamic> json) => _$PersonFromJson(json);
}
