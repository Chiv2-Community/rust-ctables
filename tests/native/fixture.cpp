#include <cstddef>

struct Payload { int amount; };

struct Left {
    int x;
    int reserved = 0;
    explicit Left(int x) : x(x) {}
    virtual int left_value() const { return x; }
    virtual int untouched() const { return x * 2; }
    virtual int apply(const Payload* payload) const { return x + payload->amount; }
};

struct Right {
    int y;
    int reserved = 0;
    explicit Right(int y) : y(y) {}
    virtual int right_value() const { return y; }
    virtual void set_right(int value) { y = value; }
};

struct Derived : Left, Right {
    int extra;
    Derived() : Left(10), Right(20), extra(3) {}
    int left_value() const override { return x + extra; }
    int right_value() const override { return y + extra; }
};

struct Grandchild : Derived {
    int left_value() const override { return x + extra + 100; }
};

struct Raw {
    int value;
    explicit Raw(int value) : value(value) {}
    virtual void skip() {}
    virtual int add(int amount) { value += amount; return value; }
    virtual int get() const { return value; }
};

struct ExtendedLeft : Left {
    ExtendedLeft() : Left(10) {}
    virtual int left_extra(int amount) const { return x + amount; }
};

struct ExtendedRight : Right {
    ExtendedRight() : Right(20) {}
    virtual int right_extra(int amount) { y += amount; return y; }
};

struct Extended : ExtendedLeft, ExtendedRight {
    int extra = 3;
    int left_extra(int amount) const override { return x + amount + extra; }
    int right_extra(int amount) override { y += amount; return y + extra; }
};

struct ExtendedGrandchild : Extended {
    virtual int grandchild_value() const { return x + y + extra; }
};

extern "C" {
Derived* fixture_new() { return new Derived; }
void fixture_delete(Derived* value) { value->~Derived(); ::operator delete(value); }
Grandchild* fixture_grandchild() { return new Grandchild; }
void fixture_delete_grandchild(Grandchild* value) { value->~Grandchild(); ::operator delete(value); }
Raw* fixture_raw() { return new Raw(7); }
void fixture_delete_raw(Raw* value) { value->~Raw(); ::operator delete(value); }
std::size_t fixture_derived_size() { return sizeof(Derived); }
std::size_t fixture_derived_align() { return alignof(Derived); }
std::size_t fixture_right_offset(Derived* value) {
    return reinterpret_cast<char*>(static_cast<Right*>(value)) - reinterpret_cast<char*>(value);
}
int fixture_right_value(Derived* value) { return value->right_value(); }
int fixture_left_value(Derived* value) { return value->left_value(); }
ExtendedLeft* fixture_extended_left() { return new ExtendedLeft; }
void fixture_delete_extended_left(ExtendedLeft* value) { value->~ExtendedLeft(); ::operator delete(value); }
Extended* fixture_extended() { return new Extended; }
void fixture_delete_extended(Extended* value) { value->~Extended(); ::operator delete(value); }
ExtendedGrandchild* fixture_extended_grandchild() { return new ExtendedGrandchild; }
void fixture_delete_extended_grandchild(ExtendedGrandchild* value) { value->~ExtendedGrandchild(); ::operator delete(value); }
std::size_t fixture_extended_size() { return sizeof(Extended); }
std::size_t fixture_extended_left_size() { return sizeof(ExtendedLeft); }
std::size_t fixture_extended_right_offset(Extended* value) {
    return reinterpret_cast<char*>(static_cast<ExtendedRight*>(value)) - reinterpret_cast<char*>(value);
}
int fixture_extended_right_value(Extended* value) { return value->right_value(); }
}
