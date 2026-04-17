module Compute

using LinearAlgebra
import Statistics: mean, std

export Transform, apply_transform, summarize

"""
    Transform

A linear transformation with offset.
WHY: We store the matrix and bias together so serialization
round-trips as a single unit instead of two separate arrays.
"""
struct Transform
    matrix::Matrix{Float64}
    bias::Vector{Float64}
end

"""
    apply_transform(t::Transform, x::Vector{Float64}) -> Vector{Float64}

Apply the affine transformation `t.matrix * x + t.bias`.
NOTE: Dimensions must match — this will throw on mismatch.
"""
function apply_transform(t::Transform, x::Vector{Float64})::Vector{Float64}
    return t.matrix * x .+ t.bias
end

function summarize(values::Vector{Float64})
    μ = mean(values)
    σ = std(values)
    println("mean=$(round(μ, digits=3)) std=$(round(σ, digits=3)) n=$(length(values))")
    return (mean=μ, std=σ)
end

function main()
    t = Transform([1.0 0.5; 0.5 1.0], [0.1, -0.1])
    result = apply_transform(t, [2.0, 3.0])
    summarize(result)
end

main()

end # module
