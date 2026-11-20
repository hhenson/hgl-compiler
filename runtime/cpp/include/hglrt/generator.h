// The coroutine type of a source's body (SOURCE-1): `hglrt::generator<T>`, a
// lazy sequence of the values a body gives with `co_yield`, which the
// generated code writes a source's body as (docs/cpp-target.md, "What is
// written").
//
// It is the shape of C++23's `std::generator<T>` for a value type `T`, cut to
// what a source needs: a move-only range whose input iterator resumes the
// body on to its next `co_yield`, whose `*it` is the value that `co_yield`
// gave, as an rvalue the caller may move from, and whose end is the body's
// completion. An exception the body lets out is thrown again by the call
// that resumed it, `begin()` or `++it`. Destroying the generator destroys
// the body's frame, and the locals in it, wherever the body is suspended.
//
// It is this library's own because not every standard library the target is
// built with has `std::generator`: Apple clang 21's libc++ has no
// `<generator>`. One implementation serves every platform, so that a body
// behaves the same on each.
#pragma once

#include <concepts>
#include <coroutine>
#include <cstddef>
#include <exception>
#include <iterator>
#include <memory>
#include <ranges>
#include <type_traits>
#include <utility>

namespace hglrt {

/// A lazy sequence of `T`, made by a coroutine that gives each element with
/// `co_yield` (SOURCE-1: a `fn` with a `yield` is a source). Its body does
/// not start until `begin()` is called, and it is resumed one element at a
/// time; `co_await` is not allowed in it.
template <class T>
class generator : public std::ranges::view_interface<generator<T>> {
    static_assert(std::is_object_v<T> && !std::is_const_v<T> && !std::is_volatile_v<T>,
                  "hglrt::generator<T> gives values: T is an object type, neither const nor volatile");

  public:
    class promise_type;
    class iterator;

    /// What the compiler builds a body's coroutine around: where the value
    /// of the body's last `co_yield` is, and the exception it let out.
    class promise_type {
      public:
        /// The generator that the call of a body gives, which owns its frame.
        generator get_return_object() noexcept {
            return generator{std::coroutine_handle<promise_type>::from_promise(*this)};
        }

        /// The body starts suspended: nothing of it runs until `begin()`.
        std::suspend_always initial_suspend() const noexcept { return {}; }

        /// The body stays suspended at its end, so that the generator, which
        /// owns the frame, destroys it.
        std::suspend_always final_suspend() const noexcept { return {}; }

        /// `co_yield` of an rvalue: the body suspends with the value where it
        /// is, a temporary of the `co_yield` expression, which lives until
        /// the body is resumed.
        std::suspend_always yield_value(T&& value) noexcept {
            yielded_ = std::addressof(value);
            return {};
        }

        /// What `co_yield` of an lvalue suspends on: a copy of it, held in
        /// the frame until the body is resumed, so that the caller may move
        /// from the copy and leave the body's variable as it was.
        struct copied {
            T value;
            bool await_ready() const noexcept { return false; }
            void await_suspend(std::coroutine_handle<promise_type> body) noexcept {
                body.promise().yielded_ = std::addressof(value);
            }
            void await_resume() const noexcept {}
        };

        /// `co_yield` of an lvalue: the body suspends with a copy of it.
        copied yield_value(const T& value) noexcept(std::is_nothrow_copy_constructible_v<T>)
            requires std::copy_constructible<T>
        {
            return copied{value};
        }

        /// The body completes: it has given its last element.
        void return_void() const noexcept {}

        /// The body lets an exception out: it is kept, and the call that
        /// resumed the body throws it again. The body is then at its end.
        void unhandled_exception() noexcept { failure_ = std::current_exception(); }

        /// No `co_await` in a body: a source suspends only at a `yield`.
        template <class Awaited>
        std::suspend_never await_transform(Awaited&&) = delete;

      private:
        friend class generator;
        friend class iterator;

        /// Resumes the body on to its next `co_yield` or its end, and throws
        /// again what it let out on the way. A body at its end is not
        /// resumed again.
        void resume() {
            const auto body = std::coroutine_handle<promise_type>::from_promise(*this);
            if (body.done()) {
                return;
            }
            body.resume();
            if (failure_) {
                std::rethrow_exception(std::exchange(failure_, nullptr));
            }
        }

        T* yielded_ = nullptr;
        std::exception_ptr failure_{};
    };

    /// The input iterator of a generator: at the body's last `co_yield`, or
    /// at its end, which is equal to `std::default_sentinel`. It is
    /// move-only, as the body it resumes can be stepped through once.
    class iterator {
      public:
        using value_type = T;
        using difference_type = std::ptrdiff_t;

        iterator(iterator&& other) noexcept : body_(std::exchange(other.body_, nullptr)) {}
        iterator& operator=(iterator&& other) noexcept {
            body_ = std::exchange(other.body_, nullptr);
            return *this;
        }

        /// The value of the body's last `co_yield`, as an rvalue: the caller
        /// may move it out. Not at the end.
        T&& operator*() const noexcept { return std::move(*body_.promise().yielded_); }

        /// Resumes the body on to its next `co_yield` or its end; what the
        /// body lets out is thrown from here. At the end it does nothing.
        iterator& operator++() {
            body_.promise().resume();
            return *this;
        }

        /// The same as `++it`.
        void operator++(int) { ++*this; }

        /// True where the body has completed, or let an exception out.
        friend bool operator==(const iterator& at, std::default_sentinel_t) noexcept {
            return at.body_.done();
        }

      private:
        friend class generator;
        explicit iterator(std::coroutine_handle<promise_type> body) noexcept : body_(body) {}

        std::coroutine_handle<promise_type> body_;
    };

    generator(const generator&) = delete;
    generator(generator&& other) noexcept : body_(std::exchange(other.body_, nullptr)) {}

    /// Takes the frame `other` owns, and destroys the one this owned.
    generator& operator=(generator other) noexcept {
        std::swap(body_, other.body_);
        return *this;
    }

    /// Destroys the body's frame, and the locals in it, wherever it is
    /// suspended.
    ~generator() {
        if (body_) {
            body_.destroy();
        }
    }

    /// Runs the body on to its first `co_yield` or its end, and gives the
    /// iterator there; what the body lets out is thrown from here. It is
    /// called once, on a generator that has not been moved from, as
    /// `std::generator::begin` is.
    iterator begin() {
        body_.promise().resume();
        return iterator{body_};
    }

    /// The end of every generator.
    std::default_sentinel_t end() const noexcept { return std::default_sentinel; }

  private:
    explicit generator(std::coroutine_handle<promise_type> body) noexcept : body_(body) {}

    std::coroutine_handle<promise_type> body_;
};

} // namespace hglrt
