#include <iostream>
#include <vector>
#include <string>
#include <memory>

namespace compute {

// WHY: Abstract base lets us swap CPU/GPU backends at runtime via config.
class Backend {
public:
    virtual ~Backend() = default;
    virtual void execute(const std::vector<float>& data) = 0;
    virtual std::string name() const = 0;
};

class CpuBackend : public Backend {
public:
    void execute(const std::vector<float>& data) override {
        // NOTE: data.size() < 1024 is the break-even point vs GPU
        std::cout << "CPU processing " << data.size() << " elements\n";
    }
    std::string name() const override { return "cpu"; }
};

class Engine {
    std::unique_ptr<Backend> backend_;
public:
    explicit Engine(std::unique_ptr<Backend> b) : backend_(std::move(b)) {}

    void run(const std::vector<float>& input) {
        std::cout << "Engine using backend: " << backend_->name() << "\n";
        backend_->execute(input);
    }
};

} // namespace compute

int main() {
    auto backend = std::make_unique<compute::CpuBackend>();
    compute::Engine engine(std::move(backend));
    engine.run({1.0f, 2.0f, 3.0f});
    return 0;
}
