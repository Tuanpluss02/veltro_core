import 'package:veltro/veltro.dart';

part 'nested.g.dart';

@Veltro()
abstract class Address with _$Address {
  const factory Address({required String street, required String city}) =
      _Address;
}

@Veltro()
abstract class Person with _$Person {
  const factory Person({required String name, required Address address}) =
      _Person;
}
