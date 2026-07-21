module MyFixture

function greet(name::String)::String
    return "Hello, $name!"
end

function add(a::Int, b::Int)::Int
    return a + b
end

function is_positive(x::Int)::Bool
    return x > 0
end

end # module